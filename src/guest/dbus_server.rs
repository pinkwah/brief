use std::ffi::OsString;

use crate::Config;
use tokio::process::Command;
use zbus::dbus_interface;

pub struct GuestServer {
    pub envs: Vec<(OsString, OsString)>,
    pub config: Config,
}

unsafe impl Sync for GuestServer {}

#[dbus_interface(name = "pink.wah.NixboxGuest1")]
impl GuestServer {
    async fn run(&self, name: &str, args: Vec<String>) -> String {
        match Command::new(name).env_clear().envs(self.envs.clone()).args(args).spawn() {
            Err(err) => format!("could not spawn: {}", err),
            Ok(mut child) => {
                let status = child.wait().await.unwrap();
                format!("exited with code {}", status)
            }
        }
    }
}
