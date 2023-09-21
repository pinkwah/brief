use std::ffi::CString;

use libc;

struct Pod<T: Sized + Clone> {
    mutex: libc::pthread_mutex_t,
    cond: libc::pthread_cond_t,
    data: Option<T>,
}

const NULL: *mut libc::c_void = 0 as *mut libc::c_void;

fn check_error(context: &'static str) {
    unsafe {
        let errno = *libc::__errno_location();
        if errno != 0 {
            panic!(
                "{}: {:?}",
                context,
                CString::from_raw(libc::strerror(errno))
            );
        }
    }
}

pub struct SharedLock<T: Sized + Clone> {
    pod_ptr: *mut Pod<T>,
}

impl<T: Sized + Clone> SharedLock<T> {
    pub fn new() -> Self {
        let pod_ptr = unsafe {
            let pod = libc::mmap(
                NULL,
                std::mem::size_of::<Pod<T>>(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_ANONYMOUS,
                -1,
                0,
            ) as *mut Pod<T>;
            if pod == 0 as *mut Pod<T> {
                check_error("libc::mmap");
            }

            (*pod).data = None;

            let mut mutexattr: libc::pthread_mutexattr_t = std::mem::zeroed();
            let mut condattr: libc::pthread_condattr_t = std::mem::zeroed();

            libc::pthread_mutexattr_init(&mut mutexattr);
            libc::pthread_mutexattr_setrobust(&mut mutexattr, libc::PTHREAD_MUTEX_ROBUST);
            libc::pthread_mutexattr_setpshared(&mut mutexattr, libc::PTHREAD_PROCESS_SHARED);
            libc::pthread_mutex_init(&mut (*pod).mutex, &mutexattr);

            libc::pthread_condattr_init(&mut condattr);
            libc::pthread_condattr_setpshared(&mut condattr, libc::PTHREAD_PROCESS_SHARED);
            libc::pthread_cond_init(&mut (*pod).cond, &condattr);

            pod
        };

        Self { pod_ptr }
    }

    pub fn wait(&mut self) -> T {
        unsafe {
            let (mutex, cond, data) = self.into_parts();
            libc::pthread_mutex_lock(mutex);
            while (*data).is_none() {
                libc::pthread_cond_wait(cond, mutex);
            }
            libc::pthread_mutex_unlock(mutex);
            (*data).clone().unwrap()
        }
    }

    pub fn send_value(&mut self, value: T) {
        unsafe {
            let (mutex, cond, data) = self.into_parts();
            libc::pthread_mutex_lock(mutex);
            *data = Some(value);
            libc::pthread_cond_signal(cond);
            libc::pthread_mutex_unlock(mutex);
        }
    }

    fn into_parts(
        &mut self,
    ) -> (
        *mut libc::pthread_mutex_t,
        *mut libc::pthread_cond_t,
        &mut Option<T>,
    ) {
        unsafe {
            let mutex = &mut (*self.pod_ptr).mutex;
            let cond = &mut (*self.pod_ptr).cond;
            let data = &mut (*self.pod_ptr).data;
            (mutex, cond, data)
        }
    }
}
