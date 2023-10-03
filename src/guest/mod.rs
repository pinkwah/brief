mod bind;
mod dbus_client;
mod dbus_server;
mod setup;

pub use dbus_client::GuestClientProxy;
use nix::unistd::{fork, ForkResult, Pid};
use std::ffi::OsString;
use std::future::pending;
use std::process::exit;

use zbus::Error::NameTaken;
use zbus::{Connection, ConnectionBuilder, Result};

use crate::util::SharedLock;

async fn async_start_server(envs: Vec<(OsString, OsString)>, func: impl FnOnce()) -> ! {
    let data = dbus_server::GuestServer { envs };

    match ConnectionBuilder::session()
        .unwrap()
        .name("pink.wah.NixboxGuest")
        .unwrap()
        .serve_at("/pink/wah/NixboxGuest", data)
        .unwrap()
        .build()
        .await
    {
        Err(NameTaken) => {
            func();
            exit(0);
        }

        Ok(_) => {
            func();
            pending::<()>().await;
            exit(0);
        }

        Err(err) => {
            func();
            panic!("Error: {:?}", err);
        }
    }
}

pub fn start_server() -> Pid {
    let mut child_lock = SharedLock::<()>::new();
    match unsafe { fork() }.expect("fork failed") {
        ForkResult::Child => {
            crate::util::terminate_on_parent_death();
            let envs = setup::setup();

            crate::util::block_on(async_start_server(envs, || child_lock.send_value(())));
        }

        ForkResult::Parent { child } => {
            child_lock.wait();
            child
        }
    }
}

pub async fn client<'a>() -> Result<GuestClientProxy<'a>> {
    let connection = Connection::session().await.unwrap_or_else(|err| {
        eprintln!(
            "Could not connect to D-Bus session: {}\nIs D-Bus running?",
            err
        );
        exit(1)
    });
    GuestClientProxy::new(&connection).await
}
