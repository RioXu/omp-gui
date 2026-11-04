//! Own only this instance's subprocess trees, never processes discovered by name.
use std::{
    io,
    ops::{Deref, DerefMut},
    process::{Child, Command},
};

pub(crate) struct OwnedProcess {
    child: Child,
    #[cfg(windows)]
    job: std::os::windows::io::OwnedHandle,
}

impl OwnedProcess {
    pub fn output(command: &mut Command) -> io::Result<std::process::Output> {
        #[cfg(windows)]
        {
            use std::{io::Read, process::Stdio};
            command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut process = Self::spawn(command)?;
            let stdout = process.stdout.take().expect("piped stdout");
            let stderr = process.stderr.take().expect("piped stderr");
            std::thread::scope(|scope| {
                let out = scope.spawn(move || {
                    let mut bytes = Vec::new();
                    let mut stdout = stdout;
                    stdout.read_to_end(&mut bytes).map(|_| bytes)
                });
                let err = scope.spawn(move || {
                    let mut bytes = Vec::new();
                    let mut stderr = stderr;
                    stderr.read_to_end(&mut bytes).map(|_| bytes)
                });
                let status = process.wait();
                // CLI queries are finished with their root. Reap workers that
                // could otherwise retain stdout/stderr and block these readers.
                let _ = process.kill();
                let stdout = out
                    .join()
                    .map_err(|_| io::Error::other("OMP stdout reader panicked"))??;
                let stderr = err
                    .join()
                    .map_err(|_| io::Error::other("OMP stderr reader panicked"))??;
                Ok(std::process::Output {
                    status: status?,
                    stdout,
                    stderr,
                })
            })
        }
        #[cfg(not(windows))]
        {
            command.output()
        }
    }

    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::{io::AsRawHandle, process::CommandExt};
            use windows_sys::Win32::System::{
                JobObjects::AssignProcessToJobObject,
                Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED},
            };
            let job = windows::new_job()?;
            // Attach before user code runs, so a wrapper cannot create an
            // unowned Bun/OMP child between spawn and assignment.
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
            let child = command.spawn()?;
            let mut process = Self { child, job };
            if unsafe {
                AssignProcessToJobObject(process.job.as_raw_handle(), process.child.as_raw_handle())
            } == 0
            {
                let error = io::Error::last_os_error();
                // Assignment failed: the root is not covered by our job yet.
                let _ = process.child.kill();
                let _ = process.child.wait();
                return Err(error);
            }
            windows::resume(process.child.id())?;
            Ok(process)
        }
        #[cfg(not(windows))]
        {
            command.spawn().map(|child| Self { child })
        }
    }

    pub fn kill(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            if unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(
                    self.job.as_raw_handle(),
                    1,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            self.child.kill()
        }
    }

    #[cfg(windows)]
    pub fn reap_exited_root(&self) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::WAIT_OBJECT_0,
            System::{JobObjects::TerminateJobObject, Threading::WaitForSingleObject},
        };
        if unsafe { WaitForSingleObject(self.child.as_raw_handle(), 0) } == WAIT_OBJECT_0 {
            // A surviving worker may still hold the RPC pipes open, hiding the
            // root's exit from the reader thread. Release the entire owned tree.
            let _ = unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) };
        }
    }
}

impl Deref for OwnedProcess {
    type Target = Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}
impl DerefMut for OwnedProcess {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.child.wait();
    }
}

#[cfg(windows)]
mod windows {
    use std::{
        io,
        mem::{size_of, zeroed},
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
        ptr::null,
    };
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            JobObjects::{
                CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };

    pub(super) fn new_job() -> io::Result<OwnedHandle> {
        // Anonymous, non-inheritable: only this GUI owns the job handle. Its
        // closure (including GUI termination) ends the remaining descendants.
        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    pub(super) fn resume(pid: u32) -> io::Result<()> {
        // Stable std::process does not expose the initial thread handle. The
        // suspended child has not executed or created any additional threads.
        let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let snapshot = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut entry: THREADENTRY32 = unsafe { zeroed() };
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
        while found != 0 {
            if entry.th32OwnerProcessID == pid {
                let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                if handle.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let thread = unsafe { OwnedHandle::from_raw_handle(handle) };
                if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                return Ok(());
            }
            found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "OMP initial thread not found",
        ))
    }
}

#[cfg(all(test, windows))]
mod tests;
