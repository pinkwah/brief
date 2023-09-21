use std::process::exit;

use nix::{
    sys::signal::{kill, Signal::SIGTERM},
    unistd::Pid,
};
use zbus::dbus_interface;

pub struct HostServer {
    pub child_pid: Pid,
}

#[dbus_interface(name = "pink.wah.NixboxHost1")]
impl HostServer {
    fn quit(&self) {
        kill(self.child_pid, SIGTERM).unwrap();
        exit(0);
    }

    fn run(&mut self, name: &str) -> String {
        format!("Host: {}", name)
    }
}
