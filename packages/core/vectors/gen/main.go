// Writes ../lighter_go.json: what lighter-go signs for every transaction this crate builds, with a
// pinned clock and a recorded random stream, so the Rust signer can be held to it byte for byte.
//
// Every Schnorr nonce and generated key comes from crypto/rand.Reader, which is replaced here by a
// stream of sha256("df-lighter-core|" + n) blocks; each case records the draws it took. L1
// signatures are go-ethereum's personal_sign over the text lighter-go asks the owner to sign, by
// Hardhat account #0. EVM calls and EIP-1559 transactions are go-ethereum's ABI packing and signer.
//
//	cd packages/core/vectors/gen && go run .
package main

import (
	"crypto/ecdsa"
	cryptorand "crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/elliottech/lighter-go/client"
	"github.com/elliottech/lighter-go/signer"
	"github.com/elliottech/lighter-go/types"
	"github.com/elliottech/lighter-go/types/txtypes"
	"github.com/ethereum/go-ethereum/accounts"
	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
)

const (
	nowMs     = int64(1790000000000)
	expiredAt = nowMs + 10*60_000 - 1_000
	hardhat0  = "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
	lighterL1 = "0x3B4D794a66304F130a4Db8F2551B0070dfCf5ca7"
	robinL1   = "0x94bAB9693Ba2f6358507eFfcbd372b0660AFfF9d"
	usdc      = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
	usdg      = "0x5fc5360D0400a0Fd4f2af552ADD042D716F1d168"
)

// stream hands out sha256("df-lighter-core|" + n) blocks and keeps each read.
type stream struct {
	n     int
	buf   []byte
	reads [][]byte
}

func (s *stream) Read(p []byte) (int, error) {
	for len(s.buf) < len(p) {
		block := sha256.Sum256([]byte("df-lighter-core|" + strconv.Itoa(s.n)))
		s.n++
		s.buf = append(s.buf, block[:]...)
	}
	out := append([]byte(nil), s.buf[:len(p)]...)
	s.buf = s.buf[len(p):]
	copy(p, out)
	s.reads = append(s.reads, out)
	return len(p), nil
}

// take returns the reads since the last take, as hex.
func (s *stream) take() []string {
	draws := make([]string, 0, len(s.reads))
	for _, r := range s.reads {
		draws = append(draws, hex.EncodeToString(r))
	}
	s.reads = nil
	return draws
}

var random = &stream{}

type key struct {
	Draws      []string `json:"draws"`
	PrivateKey string   `json:"private_key"`
	PublicKey  string   `json:"public_key"`
	manager    signer.KeyManager
}

type signerRef struct {
	Network      string `json:"network"`
	ChainID      uint32 `json:"chain_id"`
	Key          string `json:"key"`
	AccountIndex int64  `json:"account_index"`
	ApiKeyIndex  uint8  `json:"api_key_index"`
}

type out struct {
	TxType        uint8  `json:"tx_type"`
	TxInfo        string `json:"tx_info"`
	TxHash        string `json:"tx_hash"`
	MessageToSign string `json:"message_to_sign,omitempty"`
	L1Sig         string `json:"l1_sig,omitempty"`
	SignedTxInfo  string `json:"signed_tx_info,omitempty"`
}

type txCase struct {
	Name       string           `json:"name"`
	Signer     signerRef        `json:"signer"`
	ExpiredAt  int64            `json:"expired_at"`
	Nonce      int64            `json:"nonce"`
	Kind       string           `json:"kind"`
	Fields     map[string]any   `json:"fields"`
	Attributes map[string]int64 `json:"attributes,omitempty"`
	Draws      []string         `json:"draws,omitempty"`
	Out        *out             `json:"out,omitempty"`
	Error      string           `json:"error,omitempty"`
}

type tokenCase struct {
	Name     string    `json:"name"`
	Signer   signerRef `json:"signer"`
	Deadline int64     `json:"deadline_s"`
	Draws    []string  `json:"draws"`
	Token    string    `json:"token"`
}

type evmCase struct {
	Name     string `json:"name"`
	Function string `json:"function"`
	Args     []any  `json:"args"`
	Calldata string `json:"calldata"`
	Tx       any    `json:"tx,omitempty"`
}

type file struct {
	Source  string          `json:"source"`
	Random  string          `json:"random"`
	NowMs   int64           `json:"now_ms"`
	Keys    map[string]*key `json:"keys"`
	Wallet  map[string]string `json:"wallet"`
	Cases   []txCase        `json:"cases"`
	Tokens  []tokenCase     `json:"tokens"`
	Evm     []evmCase       `json:"evm"`
}

var (
	keys   = map[string]*key{}
	wallet *ecdsa.PrivateKey
)

func check(err error) {
	if err != nil {
		panic(err)
	}
}

func generate(name string) {
	random.take()
	priv, pub, err := client.GenerateAPIKey()
	check(err)
	draws := random.take()
	raw, err := hexutil.Decode(priv)
	check(err)
	manager, err := signer.NewKeyManager(raw)
	check(err)
	keys[name] = &key{Draws: draws, PrivateKey: priv, PublicKey: pub, manager: manager}
}

func attrs(a *types.L2TxAttributes) map[string]int64 {
	converted := types.ConstructL2TxAttributes(a)
	if converted == nil {
		return nil
	}
	m := map[string]int64{}
	for k, v := range converted {
		m[strconv.Itoa(int(k))] = v
	}
	return m
}

func ops(s signerRef, nonce int64, a *types.L2TxAttributes) *types.TransactOpts {
	account := s.AccountIndex
	index := s.ApiKeyIndex
	return &types.TransactOpts{FromAccountIndex: &account, ApiKeyIndex: &index, ExpiredAt: expiredAt, Nonce: &nonce, TxAttributes: a}
}

// personalSign is what a wallet answers to personal_sign: r, s and v as 27 or 28.
func personalSign(message string) string {
	sig, err := crypto.Sign(accounts.TextHash([]byte(message)), wallet)
	check(err)
	sig[64] += 27
	return hexutil.Encode(sig)
}


func record(c txCase, info txtypes.TxInfo, err error, message string, setL1 func(string)) txCase {
	c.Draws = random.take()
	if err != nil {
		c.Error = err.Error()
		c.Draws = nil
		return c
	}
	txInfo, err := info.GetTxInfo()
	check(err)
	o := &out{TxType: info.GetTxType(), TxInfo: txInfo, TxHash: info.GetTxHash(), MessageToSign: message}
	if message != "" && setL1 != nil {
		o.L1Sig = personalSign(message)
		setL1(o.L1Sig)
		signed, err := info.GetTxInfo()
		check(err)
		o.SignedTxInfo = signed
	}
	c.Out = o
	return c
}

func main() {
	cryptorand.Reader = random
	var err error
	wallet, err = crypto.HexToECDSA(hardhat0)
	check(err)

	generate("a")
	generate("b")
	// Generating twice from the same stream position must give the same key: proof the stream is
	// what signs.
	{
		saved := *random
		_, pub1, _ := client.GenerateAPIKey()
		*random = saved
		_, pub2, _ := client.GenerateAPIKey()
		if pub1 != pub2 {
			panic("crypto/rand.Reader is not the stream: the Go toolchain ignores it")
		}
		random.take()
	}

	lighter := signerRef{Network: "lighter", ChainID: 304, Key: "a", AccountIndex: 100001, ApiKeyIndex: 4}
	robin := signerRef{Network: "robinhood", ChainID: 466324, Key: "a", AccountIndex: 100002, ApiKeyIndex: 254}
	sub := signerRef{Network: "lighter", ChainID: 304, Key: "b", AccountIndex: 281474976710600, ApiKeyIndex: 5}

	var cases []txCase
	expiry := nowMs + 28*86_400_000

	order := func(name string, s signerRef, nonce int64, req types.CreateOrderTxReq, a *types.L2TxAttributes) {
		random.take()
		info, err := types.ConstructCreateOrderTx(keys[s.Key].manager, s.ChainID, &req, ops(s, nonce, a))
		fields := map[string]any{
			"MarketIndex": req.MarketIndex, "ClientOrderIndex": req.ClientOrderIndex, "BaseAmount": req.BaseAmount,
			"Price": req.Price, "IsAsk": req.IsAsk, "Type": req.Type, "TimeInForce": req.TimeInForce,
			"ReduceOnly": req.ReduceOnly, "TriggerPrice": req.TriggerPrice, "OrderExpiry": req.OrderExpiry,
		}
		c := txCase{Name: name, Signer: s, ExpiredAt: expiredAt, Nonce: nonce, Kind: "CreateOrder", Fields: fields, Attributes: attrs(a)}
		cases = append(cases, record(c, txInfoOrNil(info), err, "", nil))
	}
	u8 := func(v uint8) *uint8 { return &v }
	u32 := func(v uint32) *uint32 { return &v }
	i64 := func(v int64) *int64 { return &v }

	order("limit-buy-gtt", lighter, 0, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 1790000000000, BaseAmount: 1000, Price: 834530, IsAsk: 0, Type: 0, TimeInForce: 1, OrderExpiry: expiry}, nil)
	order("limit-sell-post-only", lighter, 1, types.CreateOrderTxReq{MarketIndex: 0, ClientOrderIndex: 7, BaseAmount: 500, Price: 310000, IsAsk: 1, Type: 0, TimeInForce: 2, OrderExpiry: expiry}, nil)
	order("limit-ioc", lighter, 2, types.CreateOrderTxReq{MarketIndex: 0, ClientOrderIndex: 8, BaseAmount: 500, Price: 310000, IsAsk: 0, Type: 0, TimeInForce: 0}, nil)
	order("market-buy", lighter, 3, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 9, BaseAmount: 20, Price: 851000, IsAsk: 0, Type: 1, TimeInForce: 0}, nil)
	order("market-sell-reduce-only", lighter, 4, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 10, BaseAmount: 20, Price: 817000, IsAsk: 1, Type: 1, TimeInForce: 0, ReduceOnly: 1}, nil)
	order("stop-loss", lighter, 5, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 11, BaseAmount: 20, Price: 784000, IsAsk: 1, Type: 2, TimeInForce: 0, ReduceOnly: 1, TriggerPrice: 800000, OrderExpiry: expiry}, nil)
	order("stop-loss-limit", lighter, 6, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 12, BaseAmount: 20, Price: 799000, IsAsk: 1, Type: 3, TimeInForce: 1, ReduceOnly: 1, TriggerPrice: 800000, OrderExpiry: expiry}, nil)
	order("take-profit", lighter, 7, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 13, BaseAmount: 20, Price: 882000, IsAsk: 1, Type: 4, TimeInForce: 0, ReduceOnly: 1, TriggerPrice: 900000, OrderExpiry: expiry}, nil)
	order("take-profit-limit", lighter, 8, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 14, BaseAmount: 20, Price: 901000, IsAsk: 1, Type: 5, TimeInForce: 1, ReduceOnly: 1, TriggerPrice: 900000, OrderExpiry: expiry}, nil)
	order("twap", lighter, 9, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 15, BaseAmount: 1000, Price: 900000, IsAsk: 0, Type: 6, TimeInForce: 1, OrderExpiry: nowMs + 3_600_000}, nil)
	order("no-client-index", lighter, 10, types.CreateOrderTxReq{MarketIndex: 1, BaseAmount: 1000, Price: 834530, Type: 0, TimeInForce: 1, OrderExpiry: expiry}, nil)
	order("spot-market-2048", lighter, 11, types.CreateOrderTxReq{MarketIndex: 2048, ClientOrderIndex: 16, BaseAmount: 1000, Price: 231000, Type: 0, TimeInForce: 1, OrderExpiry: expiry}, nil)
	order("binary-option-4206", lighter, 12, types.CreateOrderTxReq{MarketIndex: 4206, ClientOrderIndex: 17, BaseAmount: 1000000, Price: 51, Type: 0, TimeInForce: 1, OrderExpiry: expiry}, nil)
	order("widest-integers", lighter, 13, types.CreateOrderTxReq{MarketIndex: (1 << 48) - 1, ClientOrderIndex: (1 << 48) - 1, BaseAmount: (1 << 48) - 1, Price: (1 << 32) - 1, IsAsk: 1, Type: 0, TimeInForce: 1, OrderExpiry: (1 << 48) - 1}, nil)
	order("integrator-fees", lighter, 14, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 18, BaseAmount: 1000, Price: 834530, Type: 0, TimeInForce: 1, OrderExpiry: expiry},
		&types.L2TxAttributes{IntegratorAccountIndex: i64(713888), IntegratorTakerFee: u32(500), IntegratorMakerFee: u32(200)})
	order("integrator-tracking-only", lighter, 15, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 19, BaseAmount: 1000, Price: 834530, Type: 0, TimeInForce: 1, OrderExpiry: expiry},
		&types.L2TxAttributes{IntegratorAccountIndex: i64(713888)})
	order("self-trade-modes", lighter, 16, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 20, BaseAmount: 1000, Price: 834530, Type: 0, TimeInForce: 1, OrderExpiry: expiry},
		&types.L2TxAttributes{SelfTradeBehaviorMode: u8(2), SelfTradeEqualityMode: u8(1)})
	order("skip-nonce", lighter, 1000, types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 21, BaseAmount: 1000, Price: 834530, Type: 0, TimeInForce: 1, OrderExpiry: expiry},
		&types.L2TxAttributes{SkipNonce: u8(1)})
	order("robinhood-limit", robin, 0, types.CreateOrderTxReq{MarketIndex: 16, ClientOrderIndex: 1, BaseAmount: 100, Price: 25012, IsAsk: 0, Type: 0, TimeInForce: 1, OrderExpiry: expiry}, nil)
	order("sub-account-market", sub, 3, types.CreateOrderTxReq{MarketIndex: 0, ClientOrderIndex: 22, BaseAmount: 50, Price: 320000, IsAsk: 0, Type: 1, TimeInForce: 0}, nil)
	order("refused-market-255", lighter, 0, types.CreateOrderTxReq{MarketIndex: 255, BaseAmount: 1, Price: 1, Type: 0, TimeInForce: 0}, nil)
	order("refused-zero-size", lighter, 0, types.CreateOrderTxReq{MarketIndex: 1, BaseAmount: 0, Price: 1, Type: 0, TimeInForce: 0}, nil)
	order("refused-market-gtt", lighter, 0, types.CreateOrderTxReq{MarketIndex: 1, BaseAmount: 1, Price: 1, Type: 1, TimeInForce: 1, OrderExpiry: expiry}, nil)
	order("refused-fees-and-self-trade", lighter, 0, types.CreateOrderTxReq{MarketIndex: 1, BaseAmount: 1, Price: 1, Type: 0, TimeInForce: 0},
		&types.L2TxAttributes{IntegratorAccountIndex: i64(1), IntegratorTakerFee: u32(1), SelfTradeBehaviorMode: u8(1)})

	grouped := func(name string, s signerRef, nonce int64, grouping uint8, orders []types.CreateOrderTxReq, a *types.L2TxAttributes) {
		random.take()
		req := types.CreateGroupedOrdersTxReq{GroupingType: grouping}
		list := []any{}
		for i := range orders {
			o := orders[i]
			req.Orders = append(req.Orders, &o)
			list = append(list, map[string]any{
				"MarketIndex": o.MarketIndex, "ClientOrderIndex": o.ClientOrderIndex, "BaseAmount": o.BaseAmount,
				"Price": o.Price, "IsAsk": o.IsAsk, "Type": o.Type, "TimeInForce": o.TimeInForce,
				"ReduceOnly": o.ReduceOnly, "TriggerPrice": o.TriggerPrice, "OrderExpiry": o.OrderExpiry,
			})
		}
		info, err := types.ConstructL2CreateGroupedOrdersTx(keys[s.Key].manager, s.ChainID, &req, ops(s, nonce, a))
		c := txCase{Name: name, Signer: s, ExpiredAt: expiredAt, Nonce: nonce, Kind: "CreateGroupedOrders",
			Fields: map[string]any{"GroupingType": grouping, "Orders": list}, Attributes: attrs(a)}
		cases = append(cases, record(c, txInfoOrNil(info), err, "", nil))
	}
	entry := types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 30, BaseAmount: 1000, Price: 851000, IsAsk: 0, Type: 1, TimeInForce: 0}
	sl := types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 31, BaseAmount: 0, Price: 784000, IsAsk: 1, Type: 2, TimeInForce: 0, ReduceOnly: 1, TriggerPrice: 800000, OrderExpiry: expiry}
	tp := types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 32, BaseAmount: 0, Price: 882000, IsAsk: 1, Type: 4, TimeInForce: 0, ReduceOnly: 1, TriggerPrice: 900000, OrderExpiry: expiry}
	grouped("oto-entry-and-stop", lighter, 20, 1, []types.CreateOrderTxReq{entry, sl}, nil)
	grouped("otoco-entry-take-profit-stop", lighter, 21, 3, []types.CreateOrderTxReq{entry, tp, sl}, nil)
	limitEntry := types.CreateOrderTxReq{MarketIndex: 1, ClientOrderIndex: 33, BaseAmount: 1000, Price: 834530, IsAsk: 0, Type: 0, TimeInForce: 1, OrderExpiry: expiry}
	grouped("otoco-limit-entry", lighter, 22, 3, []types.CreateOrderTxReq{limitEntry, tp, sl}, nil)
	ocoTp := tp
	ocoTp.BaseAmount = 1000
	ocoSl := sl
	ocoSl.BaseAmount = 1000
	grouped("oco-take-profit-and-stop", lighter, 23, 2, []types.CreateOrderTxReq{ocoTp, ocoSl}, nil)
	grouped("otoco-integrator", lighter, 24, 3, []types.CreateOrderTxReq{entry, tp, sl},
		&types.L2TxAttributes{IntegratorAccountIndex: i64(713888), IntegratorTakerFee: u32(500)})
	grouped("refused-oto-sized-child", lighter, 0, 1, []types.CreateOrderTxReq{entry, ocoSl}, nil)

	simple := func(kind string, name string, s signerRef, nonce int64, fields map[string]any, a *types.L2TxAttributes,
		build func(signer.KeyManager, uint32, *types.TransactOpts) (txtypes.TxInfo, error), message func(txtypes.TxInfo) (string, func(string))) {
		random.take()
		info, err := build(keys[s.Key].manager, s.ChainID, ops(s, nonce, a))
		c := txCase{Name: name, Signer: s, ExpiredAt: expiredAt, Nonce: nonce, Kind: kind, Fields: fields, Attributes: attrs(a)}
		msg, set := "", func(string) {}
		if err == nil && message != nil {
			msg, set = message(info)
		}
		cases = append(cases, record(c, info, err, msg, set))
	}

	cancel := func(name string, s signerRef, nonce int64, market, index int64) {
		simple("CancelOrder", name, s, nonce, map[string]any{"MarketIndex": market, "Index": index}, nil,
			func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
				return orNil(types.ConstructL2CancelOrderTx(k, chain, &types.CancelOrderTxReq{MarketIndex: market, Index: index}, o))
			}, nil)
	}
	cancel("cancel-by-client-index", lighter, 30, 1, 1790000000000)
	cancel("cancel-by-order-index", lighter, 31, 1, 562953635421737)
	cancel("cancel-order-index-above-2^53", lighter, 32, 92, 26177173101967466)

	cancelAll := func(name string, s signerRef, nonce int64, tif uint8, at int64, a *types.L2TxAttributes) {
		simple("CancelAllOrders", name, s, nonce, map[string]any{"TimeInForce": tif, "Time": at}, a,
			func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
				return orNil(types.ConstructL2CancelAllOrdersTx(k, chain, &types.CancelAllOrdersTxReq{TimeInForce: tif, Time: at}, o))
			}, nil)
	}
	cancelAll("cancel-all-now", lighter, 33, 0, 0, nil)
	cancelAll("cancel-all-one-market", lighter, 34, 0, 0, &types.L2TxAttributes{CancelAllMarketIndex: i64(1)})
	cancelAll("cancel-all-scheduled", lighter, 35, 1, nowMs+15*60_000, nil)
	cancelAll("cancel-all-abort", lighter, 36, 2, 0, nil)
	cancelAll("refused-cancel-one-market-scheduled", lighter, 0, 1, nowMs+15*60_000, &types.L2TxAttributes{CancelAllMarketIndex: i64(1)})

	modify := func(name string, s signerRef, nonce int64, req types.ModifyOrderTxReq, a *types.L2TxAttributes) {
		simple("ModifyOrder", name, s, nonce, map[string]any{"MarketIndex": req.MarketIndex, "Index": req.Index, "BaseAmount": req.BaseAmount, "Price": req.Price, "TriggerPrice": req.TriggerPrice}, a,
			func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
				return orNil(types.ConstructL2ModifyOrderTx(k, chain, &req, o))
			}, nil)
	}
	modify("modify-limit", lighter, 37, types.ModifyOrderTxReq{MarketIndex: 1, Index: 1790000000000, BaseAmount: 2000, Price: 834000}, nil)
	modify("modify-trigger", lighter, 38, types.ModifyOrderTxReq{MarketIndex: 1, Index: 11, BaseAmount: 20, Price: 780000, TriggerPrice: 795000}, nil)
	modify("modify-with-version", lighter, 39, types.ModifyOrderTxReq{MarketIndex: 1, Index: 562953635421737, BaseAmount: 2000, Price: 834000}, &types.L2TxAttributes{OrderVersion: i64(7)})

	leverage := func(name string, s signerRef, nonce int64, market int64, fraction uint16, mode uint8) {
		simple("UpdateLeverage", name, s, nonce, map[string]any{"MarketIndex": market, "InitialMarginFraction": fraction, "MarginMode": mode}, nil,
			func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
				return orNil(types.ConstructUpdateLeverageTx(k, chain, &types.UpdateLeverageTxReq{MarketIndex: market, InitialMarginFraction: fraction, MarginMode: mode}, o))
			}, nil)
	}
	leverage("leverage-cross-10x", lighter, 40, 1, 1000, 0)
	leverage("leverage-isolated-3x", lighter, 41, 0, 3333, 1)
	leverage("refused-leverage-zero", lighter, 0, 0, 0, 0)

	margin := func(name string, s signerRef, nonce int64, market, amount int64, direction uint8) {
		simple("UpdateMargin", name, s, nonce, map[string]any{"MarketIndex": market, "USDCAmount": amount, "Direction": direction}, nil,
			func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
				return orNil(types.ConstructUpdateMarginTx(k, chain, &types.UpdateMarginTxReq{MarketIndex: market, USDCAmount: amount, Direction: direction}, o))
			}, nil)
	}
	margin("margin-add", lighter, 42, 0, 25_000_000, 1)
	margin("margin-remove", lighter, 43, 0, 5_000_000, 0)
	margin("margin-above-2^32", lighter, 44, 0, 12_345_678_901_234, 1)

	transfer := func(name string, s signerRef, nonce int64, req types.TransferTxReq) {
		simple("Transfer", name, s, nonce, map[string]any{"ToAccountIndex": req.ToAccountIndex, "AssetIndex": req.AssetIndex, "FromRouteType": req.FromRouteType,
			"ToRouteType": req.ToRouteType, "Amount": req.Amount, "USDCFee": req.USDCFee, "Memo": hex.EncodeToString(req.Memo[:])}, nil,
			func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
				return orNil(types.ConstructTransferTx(k, chain, &req, o))
			}, func(info txtypes.TxInfo) (string, func(string)) {
				t := info.(*txtypes.L2TransferTxInfo)
				return t.GetL1SignatureBody(s.ChainID), func(sig string) { t.L1Sig = sig }
			})
	}
	var memo [32]byte
	transfer("transfer-to-sub-account", lighter, 45, types.TransferTxReq{ToAccountIndex: 281474976710600, AssetIndex: 3, Amount: 25_000_000})
	transfer("transfer-to-another-owner", lighter, 46, types.TransferTxReq{ToAccountIndex: 713888, AssetIndex: 3, Amount: 100_000_000, USDCFee: 1_000_000, Memo: memo})
	fast := memo
	copy(fast[:20], common.HexToAddress("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266").Bytes())
	transfer("fast-withdraw-memo", lighter, 47, types.TransferTxReq{ToAccountIndex: 281474976710646, AssetIndex: 3, Amount: 50_000_000, USDCFee: 3_000_000, Memo: fast})
	transfer("transfer-spot-eth", lighter, 48, types.TransferTxReq{ToAccountIndex: 281474976710600, AssetIndex: 1, FromRouteType: 1, ToRouteType: 1, Amount: 12_345_678_901_234_567})
	transfer("transfer-perps-to-spot", lighter, 49, types.TransferTxReq{ToAccountIndex: 100001, AssetIndex: 3, FromRouteType: 0, ToRouteType: 1, Amount: 7_000_000})
	transfer("robinhood-transfer", robin, 1, types.TransferTxReq{ToAccountIndex: 18971, AssetIndex: 3, Amount: 2_000_000, USDCFee: 1_000_000})
	transfer("refused-transfer-zero", lighter, 0, types.TransferTxReq{ToAccountIndex: 1, AssetIndex: 3, Amount: 0})

	withdraw := func(name string, s signerRef, nonce int64, req types.WithdrawTxReq) {
		simple("Withdraw", name, s, nonce, map[string]any{"AssetIndex": req.AssetIndex, "RouteType": req.RouteType, "Amount": req.Amount}, nil,
			func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
				return orNil(types.ConstructWithdrawTx(k, chain, &req, o))
			}, nil)
	}
	withdraw("withdraw-usdc-perps", lighter, 50, types.WithdrawTxReq{AssetIndex: 3, RouteType: 0, Amount: 10_000_000})
	withdraw("withdraw-eth-spot", lighter, 51, types.WithdrawTxReq{AssetIndex: 1, RouteType: 1, Amount: 98_765_432_109_876_543})
	withdraw("robinhood-withdraw", robin, 2, types.WithdrawTxReq{AssetIndex: 3, RouteType: 0, Amount: 1_000_000})

	changePubKey := func(name string, s signerRef, nonce int64, keyName string) {
		var pk [40]byte
		raw, err := hexutil.Decode(keys[keyName].PublicKey)
		check(err)
		copy(pk[:], raw)
		simple("ChangePubKey", name, s, nonce, map[string]any{"PubKey": keys[keyName].PublicKey}, nil,
			func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
				return orNil(types.ConstructChangePubKeyTx(k, chain, &types.ChangePubKeyReq{PubKey: pk}, o))
			}, func(info txtypes.TxInfo) (string, func(string)) {
				t := info.(*txtypes.L2ChangePubKeyTxInfo)
				return t.GetL1SignatureBody(), func(sig string) { t.L1Sig = sig }
			})
	}
	changePubKey("register-key-a", lighter, 0, "a")
	changePubKey("register-key-b-on-sub", sub, 0, "b")
	changePubKey("robinhood-register", robin, 0, "a")

	plain := func(kind, name string, s signerRef, nonce int64, fields map[string]any, build func(signer.KeyManager, uint32, *types.TransactOpts) (txtypes.TxInfo, error)) {
		simple(kind, name, s, nonce, fields, nil, build, nil)
	}
	plain("CreateSubAccount", "create-sub-account", lighter, 52, map[string]any{}, func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
		return orNil(types.ConstructCreateSubAccountTx(k, chain, o))
	})
	plain("CreatePublicPool", "create-public-pool", lighter, 53, map[string]any{"OperatorFee": 100_000, "InitialTotalShares": 1_000_000_000, "MinOperatorShareRate": 1000},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructCreatePublicPoolTx(k, chain, &types.CreatePublicPoolTxReq{OperatorFee: 100_000, InitialTotalShares: 1_000_000_000, MinOperatorShareRate: 1000}, o))
		})
	plain("UpdatePublicPool", "update-public-pool", lighter, 54, map[string]any{"PublicPoolIndex": 281474976710500, "Status": 1, "OperatorFee": 50_000, "MinOperatorShareRate": 500},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructUpdatePublicPoolTx(k, chain, &types.UpdatePublicPoolTxReq{PublicPoolIndex: 281474976710500, Status: 1, OperatorFee: 50_000, MinOperatorShareRate: 500}, o))
		})
	plain("MintShares", "mint-llp-shares", lighter, 55, map[string]any{"PublicPoolIndex": 281474976710654, "ShareAmount": 3_000_000},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructMintSharesTx(k, chain, &types.MintSharesTxReq{PublicPoolIndex: 281474976710654, ShareAmount: 3_000_000}, o))
		})
	plain("BurnShares", "burn-llp-shares", lighter, 56, map[string]any{"PublicPoolIndex": 281474976710654, "ShareAmount": 1_500_000},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructBurnSharesTx(k, chain, &types.BurnSharesTxReq{PublicPoolIndex: 281474976710654, ShareAmount: 1_500_000}, o))
		})
	plain("StakeAssets", "stake-lit", lighter, 57, map[string]any{"StakingPoolIndex": 281474976624800, "ShareAmount": 100_000_000},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructStakeAssetsTx(k, chain, &types.StakeAssetsTxReq{StakingPoolIndex: 281474976624800, ShareAmount: 100_000_000}, o))
		})
	plain("UnstakeAssets", "unstake-lit", lighter, 58, map[string]any{"StakingPoolIndex": 281474976624800, "ShareAmount": 40_000_000},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructUnstakeAssetsTx(k, chain, &types.UnstakeAssetsTxReq{StakingPoolIndex: 281474976624800, ShareAmount: 40_000_000}, o))
		})
	plain("UpdateAccountConfig", "unified-trading-mode", lighter, 59, map[string]any{"AccountTradingMode": 1},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructUpdateAccountConfigTx(k, chain, &types.UpdateAccountConfigTxReq{AccountTradingMode: 1}, o))
		})
	plain("UpdateAccountAssetConfig", "asset-as-margin", lighter, 60, map[string]any{"AssetIndex": 1, "AssetMarginMode": 1},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructUpdateAccountAssetConfigTx(k, chain, &types.UpdateAccountAssetConfigTxReq{AssetIndex: 1, AssetMarginMode: 1}, o))
		})

	approve := func(name string, s signerRef, nonce int64, req types.ApproveIntegratorTxReq) {
		simple("ApproveIntegrator", name, s, nonce, map[string]any{"IntegratorAccountIndex": req.IntegratorAccountIndex, "MaxPerpsTakerFee": req.MaxPerpsTakerFee,
			"MaxPerpsMakerFee": req.MaxPerpsMakerFee, "MaxSpotTakerFee": req.MaxSpotTakerFee, "MaxSpotMakerFee": req.MaxSpotMakerFee, "ApprovalExpiry": req.ApprovalExpiry}, nil,
			func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
				return orNil(types.ConstructApproveIntegratorTx(k, chain, &req, o))
			}, func(info txtypes.TxInfo) (string, func(string)) {
				t := info.(*txtypes.L2ApproveIntegratorTxInfo)
				return t.GetL1SignatureBody(s.ChainID), func(sig string) { t.L1Sig = sig }
			})
	}
	approve("approve-integrator", lighter, 61, types.ApproveIntegratorTxReq{IntegratorAccountIndex: 713888, MaxPerpsTakerFee: 1000, MaxPerpsMakerFee: 500, MaxSpotTakerFee: 10000, MaxSpotMakerFee: 0, ApprovalExpiry: nowMs + 365*86_400_000})
	approve("revoke-integrator", lighter, 62, types.ApproveIntegratorTxReq{IntegratorAccountIndex: 713888})
	approve("robinhood-approve-integrator", robin, 3, types.ApproveIntegratorTxReq{IntegratorAccountIndex: 45, MaxPerpsTakerFee: 10000, MaxPerpsMakerFee: 10000, MaxSpotTakerFee: 20000, MaxSpotMakerFee: 20000, ApprovalExpiry: nowMs + 30*86_400_000})

	createMarket := types.BinaryOptionsOperatorCreateMarketTxReq{MarketSlot: 1500, StartTimestampMs: nowMs + 2*3_600_000, EndTimestampMs: nowMs + 2*3_600_000 + 900_000,
		SettlementCap: 100, SettlementType: 0, DefaultPrice: 50, TakerFee: 10000, MakerFee: 0, MinBaseAmount: 50000, MinQuoteAmount: 1, OrderQuoteLimit: 281474976710655,
		OpenInterestLimit: 1 << 50, IsFrozen: 0, Symbol: "ETH-UPDOWN-15M-1790007200", SizeDecimals: 4, PriceDecimals: 2}
	plain("BinaryOptionsOperatorCreateMarket", "operator-create-market", lighter, 63, map[string]any{"MarketSlot": createMarket.MarketSlot, "StartTimestampMs": createMarket.StartTimestampMs,
		"EndTimestampMs": createMarket.EndTimestampMs, "SettlementCap": createMarket.SettlementCap, "SettlementType": createMarket.SettlementType, "DefaultPrice": createMarket.DefaultPrice,
		"TakerFee": createMarket.TakerFee, "MakerFee": createMarket.MakerFee, "MinBaseAmount": createMarket.MinBaseAmount, "MinQuoteAmount": createMarket.MinQuoteAmount,
		"OrderQuoteLimit": createMarket.OrderQuoteLimit, "OpenInterestLimit": createMarket.OpenInterestLimit, "IsFrozen": createMarket.IsFrozen, "Symbol": createMarket.Symbol,
		"SizeDecimals": createMarket.SizeDecimals, "PriceDecimals": createMarket.PriceDecimals},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructBinaryOptionsOperatorCreateMarketTx(k, chain, &createMarket, o))
		})
	updateMarket := types.BinaryOptionsOperatorUpdateMarketTxReq{MarketSlot: 1500, MarketIndex: 4206, StartTimestampMs: nowMs, EndTimestampMs: nowMs + 900_000,
		TakerFee: 10000, MakerFee: 0, MinBaseAmount: 50000, MinQuoteAmount: 1, OrderQuoteLimit: 281474976710655, OpenInterestLimit: 1 << 50, IsFrozen: 1}
	plain("BinaryOptionsOperatorUpdateMarket", "operator-freeze-market", lighter, 64, map[string]any{"MarketSlot": updateMarket.MarketSlot, "MarketIndex": updateMarket.MarketIndex,
		"StartTimestampMs": updateMarket.StartTimestampMs, "EndTimestampMs": updateMarket.EndTimestampMs, "TakerFee": updateMarket.TakerFee, "MakerFee": updateMarket.MakerFee,
		"LiquidationFee": updateMarket.LiquidationFee, "MinBaseAmount": updateMarket.MinBaseAmount, "MinQuoteAmount": updateMarket.MinQuoteAmount,
		"OrderQuoteLimit": updateMarket.OrderQuoteLimit, "OpenInterestLimit": updateMarket.OpenInterestLimit, "IsFrozen": updateMarket.IsFrozen},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructBinaryOptionsOperatorUpdateMarketTx(k, chain, &updateMarket, o))
		})
	settle := types.BinaryOptionsOperatorSettleMarketTxReq{MarketSlot: 1500, MarketIndex: 4206, SettlementPrice: 100}
	plain("BinaryOptionsOperatorSettleMarket", "operator-settle-up", lighter, 65, map[string]any{"MarketSlot": settle.MarketSlot, "MarketIndex": settle.MarketIndex,
		"SettlementPrice": settle.SettlementPrice, "IsRefund": settle.IsRefund},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructBinaryOptionsOperatorSettleMarketTx(k, chain, &settle, o))
		})
	refund := types.BinaryOptionsOperatorSettleMarketTxReq{MarketSlot: 1500, MarketIndex: 4206, IsRefund: 1}
	plain("BinaryOptionsOperatorSettleMarket", "operator-refund", lighter, 66, map[string]any{"MarketSlot": refund.MarketSlot, "MarketIndex": refund.MarketIndex,
		"SettlementPrice": refund.SettlementPrice, "IsRefund": refund.IsRefund},
		func(k signer.KeyManager, chain uint32, o *types.TransactOpts) (txtypes.TxInfo, error) {
			return orNil(types.ConstructBinaryOptionsOperatorSettleMarketTx(k, chain, &refund, o))
		})

	var tokens []tokenCase
	token := func(name string, s signerRef, deadline int64) {
		random.take()
		account, index := s.AccountIndex, s.ApiKeyIndex
		t, err := types.ConstructAuthToken(keys[s.Key].manager, time.Unix(deadline, 0), &types.TransactOpts{FromAccountIndex: &account, ApiKeyIndex: &index})
		check(err)
		tokens = append(tokens, tokenCase{Name: name, Signer: s, Deadline: deadline, Draws: random.take(), Token: t})
	}
	token("auth-token-7h", lighter, nowMs/1000+7*3600)
	token("auth-token-robinhood", robin, nowMs/1000+3600)
	token("auth-token-sub-account", sub, nowMs/1000+600)

	out := file{
		Source: "lighter-go 01933021 (types.Construct*Tx, types.ConstructAuthToken, client.GenerateAPIKey) and go-ethereum 1.15.6 (accounts.TextHash, crypto.Sign, abi, types.SignTx), run natively by vectors/gen",
		Random: "crypto/rand.Reader replaced by sha256(\"df-lighter-core|\" + n) blocks; each case lists the reads it took, as hex",
		NowMs:  nowMs,
		Keys:   keys,
		Wallet: map[string]string{"private_key": "0x" + hardhat0, "address": crypto.PubkeyToAddress(wallet.PublicKey).Hex()},
		Cases:  cases,
		Tokens: tokens,
		Evm:    evm(),
	}
	data, err := json.MarshalIndent(out, "", "  ")
	check(err)
	check(os.WriteFile("../lighter_go.json", append(data, '\n'), 0o644))
	fmt.Printf("%d cases, %d tokens, %d evm\n", len(cases), len(tokens), len(out.Evm))
}

func orNil[T txtypes.TxInfo](info T, err error) (txtypes.TxInfo, error) {
	if err != nil {
		return nil, err
	}
	return info, nil
}

func txInfoOrNil[T txtypes.TxInfo](info T) txtypes.TxInfo {
	var zero T
	if any(info) == any(zero) {
		return nil
	}
	return info
}

const lighterAbi = `[
 {"type":"function","name":"deposit","stateMutability":"payable","inputs":[{"name":"_to","type":"address"},{"name":"_assetIndex","type":"uint16"},{"name":"_routeType","type":"uint8"},{"name":"_amount","type":"uint256"}],"outputs":[]},
 {"type":"function","name":"withdraw","stateMutability":"nonpayable","inputs":[{"name":"accountIndex","type":"uint48"},{"name":"assetIndex","type":"uint16"},{"name":"routeType","type":"uint8"},{"name":"amount","type":"uint64"}],"outputs":[]},
 {"type":"function","name":"changePubKey","stateMutability":"nonpayable","inputs":[{"name":"accountIndex","type":"uint48"},{"name":"apiKeyIndex","type":"uint8"},{"name":"pubKey","type":"bytes"}],"outputs":[]},
 {"type":"function","name":"cancelAllOrders","stateMutability":"nonpayable","inputs":[{"name":"accountIndex","type":"uint48"}],"outputs":[]},
 {"type":"function","name":"createOrder","stateMutability":"nonpayable","inputs":[{"name":"accountIndex","type":"uint48"},{"name":"marketIndex","type":"uint16"},{"name":"baseAmount","type":"uint48"},{"name":"price","type":"uint32"},{"name":"isAsk","type":"uint8"},{"name":"orderType","type":"uint8"}],"outputs":[]},
 {"type":"function","name":"burnShares","stateMutability":"nonpayable","inputs":[{"name":"accountIndex","type":"uint48"},{"name":"publicPoolIndex","type":"uint48"},{"name":"shareAmount","type":"uint64"}],"outputs":[]},
 {"type":"function","name":"withdrawPendingBalance","stateMutability":"nonpayable","inputs":[{"name":"owner","type":"address"},{"name":"assetIndex","type":"uint16"},{"name":"amount","type":"uint128"}],"outputs":[]},
 {"type":"function","name":"addressToAccountIndex","stateMutability":"view","inputs":[{"name":"owner","type":"address"}],"outputs":[{"type":"uint48"}]},
 {"type":"function","name":"getPendingBalance","stateMutability":"view","inputs":[{"name":"owner","type":"address"},{"name":"assetIndex","type":"uint16"}],"outputs":[{"type":"uint128"}]},
 {"type":"function","name":"tokenToAssetIndex","stateMutability":"view","inputs":[{"name":"token","type":"address"}],"outputs":[{"type":"uint16"}]},
 {"type":"function","name":"approve","stateMutability":"nonpayable","inputs":[{"name":"spender","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[{"type":"bool"}]},
 {"type":"function","name":"allowance","stateMutability":"view","inputs":[{"name":"owner","type":"address"},{"name":"spender","type":"address"}],"outputs":[{"type":"uint256"}]},
 {"type":"function","name":"balanceOf","stateMutability":"view","inputs":[{"name":"owner","type":"address"}],"outputs":[{"type":"uint256"}]}
]`

func evm() []evmCase {
	parsed, err := abi.JSON(strings.NewReader(lighterAbi))
	check(err)
	owner := crypto.PubkeyToAddress(wallet.PublicKey)
	var cases []evmCase
	call := func(name, function string, show []any, args ...any) []byte {
		data, err := parsed.Pack(function, args...)
		check(err)
		cases = append(cases, evmCase{Name: name, Function: parsed.Methods[function].Sig, Args: show, Calldata: hexutil.Encode(data)})
		return data
	}
	amount := big.NewInt(25_500_000)
	deposit := call("deposit-usdc-perps", "deposit", []any{owner.Hex(), 3, 0, "25500000"}, owner, uint16(3), uint8(0), amount)
	call("deposit-eth-spot", "deposit", []any{owner.Hex(), 1, 1, "50000000000000000"}, owner, uint16(1), uint8(1), big.NewInt(50_000_000_000_000_000))
	call("approve-lighter", "approve", []any{lighterL1, "25500000"}, common.HexToAddress(lighterL1), amount)
	call("withdraw-usdc", "withdraw", []any{10, 3, 0, "10000000"}, big.NewInt(10), uint16(3), uint8(0), uint64(10_000_000))
	pk, err := hexutil.Decode(keys["a"].PublicKey)
	check(err)
	call("change-pub-key", "changePubKey", []any{100001, 4, keys["a"].PublicKey}, big.NewInt(100001), uint8(4), pk)
	call("cancel-all-orders", "cancelAllOrders", []any{100001}, big.NewInt(100001))
	call("create-order", "createOrder", []any{100001, 1, 20, 817000, 1, 1}, big.NewInt(100001), uint16(1), big.NewInt(20), uint32(817000), uint8(1), uint8(1))
	call("burn-shares", "burnShares", []any{100001, "281474976710654", 1500000}, big.NewInt(100001), big.NewInt(281474976710654), uint64(1_500_000))
	call("withdraw-pending-balance", "withdrawPendingBalance", []any{owner.Hex(), 3, "10000000"}, owner, uint16(3), big.NewInt(10_000_000))
	call("address-to-account-index", "addressToAccountIndex", []any{owner.Hex()}, owner)
	call("get-pending-balance", "getPendingBalance", []any{owner.Hex(), 3}, owner, uint16(3))
	call("token-to-asset-index", "tokenToAssetIndex", []any{usdc}, common.HexToAddress(usdc))
	call("allowance", "allowance", []any{owner.Hex(), lighterL1}, owner, common.HexToAddress(lighterL1))
	call("balance-of", "balanceOf", []any{owner.Hex()}, owner)

	sign := func(name string, chain int64, nonce uint64, to string, data []byte, value *big.Int) {
		toAddr := common.HexToAddress(to)
		tx := ethtypes.NewTx(&ethtypes.DynamicFeeTx{ChainID: big.NewInt(chain), Nonce: nonce, GasTipCap: big.NewInt(1_000_000_000),
			GasFeeCap: big.NewInt(30_000_000_000), Gas: 120_000, To: &toAddr, Value: value, Data: data})
		signed, err := ethtypes.SignTx(tx, ethtypes.LatestSignerForChainID(big.NewInt(chain)), wallet)
		check(err)
		raw, err := signed.MarshalBinary()
		check(err)
		cases = append(cases, evmCase{Name: name, Function: "eip1559", Args: []any{chain, nonce, "1000000000", "30000000000", 120000, to, value.String(), hexutil.Encode(data)},
			Calldata: hexutil.Encode(data), Tx: map[string]string{"raw": hexutil.Encode(raw), "hash": signed.Hash().Hex()}})
	}
	sign("deposit-tx-ethereum", 1, 7, lighterL1, deposit, big.NewInt(0))
	usdgApprove, err := parsed.Pack("approve", common.HexToAddress(robinL1), amount)
	check(err)
	sign("approve-tx-robinhood", 4663, 0, usdg, usdgApprove, big.NewInt(0))
	sign("deposit-tx-robinhood", 4663, 1, robinL1, deposit, big.NewInt(0))
	return cases
}
