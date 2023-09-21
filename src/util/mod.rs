mod resolve_symlink;
mod shlock;

use std::future::Future;

pub use resolve_symlink::resolve_symlink;
pub use shlock::SharedLock;

pub fn terminate_on_parent_death() {
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
    }
}

pub fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}
