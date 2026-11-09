#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("FRITH_DEMO_PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse::<u16>()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    println!("Frith mock workspace: http://{}", listener.local_addr()?);
    axum::serve(listener, frith_web::app()).await?;
    Ok(())
}
