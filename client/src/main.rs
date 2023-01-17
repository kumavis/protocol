mod sign;
mod transaction;

#[tokio::main]
async fn main() {
	sign::signs().await.unwrap();
	transaction::transfers().await.unwrap();
}
