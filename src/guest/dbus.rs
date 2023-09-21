use std::{error::Error, future::pending};

use zbus::{ConnectionBuilder, dbus_interface};
use super::Guest;

struct DBusGuest {
    guest: Guest
}

#[dbus_interface(name = "pink.wah.NixboxGuest1")]
impl DBusGuest {
    fn run(&mut self, name: &str) -> String {
        format!("Name: {}", name)
    }
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let guest = Guest::new();
    let dbus = DBusGuest { guest };
    ConnectionBuilder::session()?
        .name("pink.wah.NixboxGuest")?
        .serve_at("/pink/wah/NixboxGuest", dbus)?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
