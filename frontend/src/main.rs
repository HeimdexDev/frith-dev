#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("FRITH_DEMO_PORT")
        .or_else(|_| std::env::var("PORT"))
        .unwrap_or_else(|_| "3000".into())
        .parse::<u16>()?;
    let bind = std::env::var("FRITH_DEMO_BIND")
        .unwrap_or_else(|_| "127.0.0.1".into())
        .parse::<std::net::IpAddr>()?;
    let listener = tokio::net::TcpListener::bind((bind, port)).await?;
    println!("Frith mock workspace: http://{}", listener.local_addr()?);
    let config = frith_web::processing::GpuConfig::from_env()?;
    axum::serve(
        listener,
        frith_web::app_with_gpu(Box::new(frith_web::api::MockApi::default()), config),
    )
    .await?;
    Ok(())
}
