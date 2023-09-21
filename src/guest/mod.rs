mod bind;
mod dbus_client;
mod dbus_server;
mod setup;

pub use dbus_client::GuestClientProxy;
use nix::unistd::{fork, ForkResult, Pid};
use std::env;
use std::ffi::OsString;
use std::future::pending;
use std::process::exit;

use zbus::Error::NameTaken;
use zbus::{Connection, ConnectionBuilder, Result};

use crate::util::SharedLock;
use crate::Config;

async fn async_start_server(mut envs: Vec<(OsString, OsString)>, config: &Config, func: impl FnOnce()) -> ! {
    envs.push((
        "NIXOS_CONFIG".into(),
        config.guest_nixos_config.as_os_str().into(),
    ));

    let data = dbus_server::GuestServer {
        envs: envs,
        config: config.clone(),
    };

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

pub fn start_server(config: &Config) -> Pid {
    let mut child_lock = SharedLock::<()>::new();
    match unsafe { fork() }.expect("fork failed") {
        ForkResult::Child => {
            crate::util::terminate_on_parent_death();
            let envs = setup::setup(&config);

            crate::util::block_on(async_start_server(envs, config, || child_lock.send_value(())));
        }

        ForkResult::Parent { child } => {
            child_lock.wait();
            child
        }
    }
}

pub async fn client<'a>() -> Result<GuestClientProxy<'a>> {
    let connection = Connection::session().await?;
    GuestClientProxy::new(&connection).await
}
