#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    master_fiber_agent::run().await
}
