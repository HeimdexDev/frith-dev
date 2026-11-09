#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("Frith mock workspace: http://127.0.0.1:3000");
    axum::serve(listener, frith_web::app()).await?;
    Ok(())
}
