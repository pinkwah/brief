use zbus::{Result, dbus_proxy};

#[dbus_proxy(
    interface = "pink.wah.NixboxGuest1",
    default_service = "pink.wah.NixboxGuest",
    default_path = "/pink/wah/NixboxGuest",
)]
trait GuestClient {
    async fn run(&self, name: &str, args: &[&str]) -> Result<String>;
}
