pub fn probe() -> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    let root = std::env::current_dir()?;
    let runtime = super::Client::start(
        "omp",
        &root,
        &crate::infrastructure::persistence::workspace_store::data_dir().join("probe-sessions"),
        None,
    )?;
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        match runtime.events.recv_timeout(Duration::from_secs(1)) {
            Ok(super::Event::Frame(v)) if v["type"] == "ready" => {
                runtime.send(serde_json::json!({"type":"negotiate_protocol","id":"probe-protocol","protocolVersion":2}))?;
                runtime.send(serde_json::json!({"type":"get_state","id":"probe-state"}))?;
            }
            Ok(super::Event::Frame(v)) if v["id"] == "probe-state" => {
                anyhow::ensure!(v["success"] == true, "OMP state request failed");
                println!(
                    "OMP handshake OK; model={}; no prompt sent",
                    v["data"]["model"]["id"].as_str().unwrap_or("unconfigured")
                );
                return Ok(());
            }
            Ok(super::Event::Error(e)) => eprintln!("{e}"),
            Ok(super::Event::Closed) => anyhow::bail!("OMP exited before handshake"),
            _ => {}
        }
    }
    anyhow::bail!("OMP handshake timed out")
}
