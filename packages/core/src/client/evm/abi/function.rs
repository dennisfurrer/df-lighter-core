//! Functions by their signature: a [`Function`]'s selector, calldata and answer.

use crate::client::evm::abi::{AbiError, Kind, Token, decode, encode, kinds};
use crate::utils::hex;
use crate::utils::sign::keccak256;

/// A function as a signature writes it: `addressToAccountIndex(address) returns (uint48)`, the
/// returns part optional, parameter names allowed and dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub inputs: Vec<Kind>,
    pub outputs: Vec<Kind>,
}

impl Function {
    pub fn parse(text: &str) -> Result<Function, AbiError> {
        let bad = || AbiError::Signature(text.trim().to_owned());
        let text = text.trim();
        let open = text.find('(').ok_or_else(bad)?;
        let name = text[..open].trim();
        if name.is_empty() || !name.chars().all(|letter| letter.is_ascii_alphanumeric() || letter == '_') {
            return Err(bad());
        }
        let close = open + text[open..].find(')').ok_or_else(bad)?;
        let inputs = kinds(&text[open + 1..close])?;
        let rest = text[close + 1..].trim();
        let outputs = match rest.strip_prefix("returns") {
            Some(after) => {
                let after = after.trim();
                let inner = after.strip_prefix('(').and_then(|after| after.strip_suffix(')')).ok_or_else(bad)?;
                kinds(inner)?
            }
            None if rest.is_empty() => Vec::new(),
            None => return Err(bad()),
        };
        Ok(Function { name: name.to_owned(), inputs, outputs })
    }

    /// `name(t1,t2)`, what the selector hashes.
    #[must_use]
    pub fn signature(&self) -> String {
        let kinds: Vec<String> = self.inputs.iter().map(Kind::canonical).collect();
        format!("{}({})", self.name, kinds.join(","))
    }

    /// The four bytes that name the function: the start of `keccak256(signature)`.
    #[must_use]
    pub fn selector(&self) -> [u8; 4] {
        let hash = keccak256(self.signature().as_bytes());
        [hash[0], hash[1], hash[2], hash[3]]
    }

    /// The calldata of a call: the selector, then the arguments.
    pub fn encode(&self, args: &[Token]) -> Result<Vec<u8>, AbiError> {
        let mut out = self.selector().to_vec();
        out.extend(encode(&self.inputs, args)?);
        Ok(out)
    }

    /// A call's arguments back out of its calldata.
    pub fn decode_input(&self, calldata: &[u8]) -> Result<Vec<Token>, AbiError> {
        let found = calldata.get(..4).ok_or(AbiError::Short(calldata.len()))?;
        if found != self.selector() {
            return Err(AbiError::Selector {
                wants: hex::encode_prefixed(&self.selector()),
                found: hex::encode_prefixed(found),
            });
        }
        decode(&self.inputs, &calldata[4..])
    }

    /// What the call answered, read against its declared outputs.
    pub fn decode_output(&self, answer: &[u8]) -> Result<Vec<Token>, AbiError> {
        decode(&self.outputs, answer)
    }
}

/// The selector of a function signature.
pub fn selector(signature: &str) -> Result<[u8; 4], AbiError> {
    Function::parse(signature).map(|function| function.selector())
}
