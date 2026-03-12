// Local stub for the optional enterprise control-plane crate.
// Allows compiling `enterprise-backend` without external deps.

#![allow(dead_code)]

#[cfg(feature = "enterprise-backend")]
pub async fn start(_tx: crate::facade::runtime_ctrl::RuntimeCommandSender) -> anyhow::Result<()> {
    // No external control plane in community builds; report disabled.
    Ok(())
}
