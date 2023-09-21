use zbus::{Result, dbus_proxy};

#[dbus_proxy(
    interface = "pink.wah.NixboxHost1",
    default_service = "pink.wah.NixboxHost",
    default_path = "/pink/wah/NixboxHost",
)]
trait HostClient {
    fn quit(&self) -> Result<()>;
    fn run(&self, name: &str) -> Result<String>;
}
