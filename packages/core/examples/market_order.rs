//! Reads BTC's details and book from Lighter and prints the unsigned market order a $20 buy
//! would send: `cargo run -p df-lighter-core --example market_order`. Name a network
//! (`-- robinhood`) to read Lighter on Robinhood Chain instead.

use std::error::Error;

use df_lighter_core::client::native::{Native, clock};
use df_lighter_core::markets::{self, MarketFilter};
use df_lighter_core::trade::{CreateOrder, OrderIntent, Side, Size};
use df_lighter_core::utils::Json;
use df_lighter_core::utils::sign::Tx;
use df_lighter_core::{Client, Dec, Network};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let network: Network = std::env::args().nth(1).as_deref().unwrap_or("lighter").parse()?;
    let client = Client::reader(Native::new(), network, clock);

    let perps = markets::order_book_details(&client, MarketFilter::Perps).await?;
    let btc = perps.by_symbol("BTC").ok_or("BTC is not listed")?;
    let book = markets::order_book_orders(&client, btc.market_id, 50).await?;
    let (bid, ask) = book.touch();
    let ask = ask.ok_or("no one is selling")?;

    println!(
        "{network}  BTC  market {}  up to {}x",
        btc.market_id,
        show(btc.max_leverage().map(|most| Dec::from_int(i64::from(most))))
    );
    println!("     size step {}  tick {}", btc.size_step(), btc.tick());
    println!("     bid {}  ask {ask}  mid {}", show(bid), show(book.mid()));

    let intent = OrderIntent::market(btc.market_id, Side::Bid, Size::Quote("20".parse()?), ask);
    let order = intent.build(btc.grid(), clock())?;
    let walk = book.walk(Side::Bid, btc.grid().size_of(order.base_amount));
    println!("     fills {} at {} on average, {} at worst", walk.filled, show(walk.average), show(walk.worst));

    let fields: Vec<(String, Json)> =
        CreateOrder::new(order).info_fields().into_iter().map(|(key, value)| (key.to_owned(), value)).collect();
    println!("\nwould sign: {}", Json::Object(fields).compact());

    let spent = client.spent();
    println!("\nspent {} of the IP's {} a minute", spent.minute, spent.budget);
    Ok(())
}

fn show(value: Option<Dec>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}
