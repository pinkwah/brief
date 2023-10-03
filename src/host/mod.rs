mod dbus_client;
mod dbus_server;

pub use dbus_client::HostClientProxy;
use nix::sys::signal::{kill, signal, SigHandler, Signal};
use nix::unistd::{fork, ForkResult, Pid};
use once_cell::sync::Lazy;
use std::convert::TryFrom;
use std::sync::Mutex;
use std::{future::pending, process::exit};

use zbus::Error::NameTaken;
use zbus::{Connection, ConnectionBuilder, Result};

static mut CHILD_PID: Lazy<Mutex<Option<Pid>>> = Lazy::new(|| Mutex::new(None));

extern "C" fn handle_quit_signal(signal: libc::c_int) {
    let signal = Signal::try_from(signal).unwrap();

    if let Ok(pid) = unsafe { CHILD_PID.lock() } {
        if let Some(pid) = *pid {
            let _ = kill(pid, signal);
        }
    }
}

fn install_quit_signal() {
    let handler = SigHandler::Handler(handle_quit_signal);
    for sig in [Signal::SIGTERM, Signal::SIGABRT] {
        unsafe {
            let _ = signal(sig, handler);
        }
    }
}

async fn async_start_server(child_pid: Pid, func: impl FnOnce()) -> ! {
    let data = dbus_server::HostServer { child_pid };

    match ConnectionBuilder::session()
        .unwrap()
        .name("pink.wah.NixboxHost")
        .unwrap()
        .serve_at("/pink/wah/NixboxHost", data)
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

pub fn start_server() -> ! {
    let child_pid = crate::guest::start_server();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_start_server(child_pid, || ()));
}

pub fn start_server_fork() -> Pid {
    let mut child_lock = crate::util::SharedLock::<()>::new();
    match unsafe { fork() }.expect("fork failed") {
        ForkResult::Child => {
            crate::util::terminate_on_parent_death();
            let child_pid = crate::guest::start_server();

            crate::util::block_on(async_start_server(child_pid, || child_lock.send_value(())));
        }
        ForkResult::Parent { child } => {
            let mut global_child_pid = unsafe { CHILD_PID.lock() }.unwrap();
            *global_child_pid = Some(child);

            install_quit_signal();
            child_lock.wait();
            child
        }
    }
}

pub async fn client<'a>() -> Result<HostClientProxy<'a>> {
    let connection = Connection::session().await?;
    HostClientProxy::new(&connection).await
}
