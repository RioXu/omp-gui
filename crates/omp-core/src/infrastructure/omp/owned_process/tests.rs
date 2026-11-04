use super::*;
use crate::infrastructure::process::hidden_command;
use std::{
    fs,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    process::Stdio,
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
};

const HELPER: &str = "infrastructure::omp::owned_process::tests::process_tree_fixture";

struct Fixture {
    directory: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("omp-process-tree-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        Self { directory }
    }
    fn command(&self, mode: &str) -> Command {
        command(mode, &self.directory)
    }
    fn ready(&self) -> (u32, u32) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(text) = fs::read_to_string(self.directory.join("ready"))
                && let Some((root, leaf)) = text.split_once(',')
                && let (Ok(root), Ok(leaf)) = (root.parse(), leaf.parse())
            {
                return (root, leaf);
            }
            assert!(Instant::now() < deadline, "process fixture did not start");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.directory.starts_with(std::env::temp_dir())
                && self
                    .directory
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("omp-process-tree-")
        );
        let _ = fs::remove_dir_all(&self.directory);
    }
}
fn command(mode: &str, directory: &Path) -> Command {
    let mut command = hidden_command(std::env::current_exe().unwrap().to_str().unwrap());
    command
        .args(["--ignored", "--exact", HELPER, "--nocapture"])
        .env("OMP_GUI_PROCESS_FIXTURE", mode)
        .env("OMP_GUI_PROCESS_FIXTURE_DIR", directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
fn watch(pid: u32) -> OwnedHandle {
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(
        !handle.is_null(),
        "fixture process unavailable: {}",
        io::Error::last_os_error()
    );
    unsafe { OwnedHandle::from_raw_handle(handle) }
}
fn assert_stopped(handle: &OwnedHandle) {
    assert_eq!(
        unsafe { WaitForSingleObject(handle.as_raw_handle(), 5000) },
        WAIT_OBJECT_0,
        "owned process outlived its owner"
    );
}

#[test]
fn dropping_transport_terminates_its_descendants_and_preserves_other_processes() {
    let fixture = Fixture::new();
    let process = OwnedProcess::spawn(&mut fixture.command("root")).unwrap();
    let (root, leaf) = fixture.ready();
    let root = watch(root);
    let leaf = watch(leaf);
    let mut unrelated = OwnedProcess::spawn(&mut fixture.command("leaf")).unwrap();
    drop(process);
    assert_stopped(&root);
    assert_stopped(&leaf);
    assert!(
        unrelated.try_wait().unwrap().is_none(),
        "another session must remain alive"
    );
}

#[test]
fn descendants_are_reaped_even_after_the_transport_root_has_exited() {
    let fixture = Fixture::new();
    let mut process = OwnedProcess::spawn(&mut fixture.command("root")).unwrap();
    let (_, leaf) = fixture.ready();
    let leaf = watch(leaf);
    // Emulate a shell/Bun wrapper exiting before its worker.
    process.child.kill().unwrap();
    process.child.wait().unwrap();
    assert_eq!(
        unsafe { WaitForSingleObject(leaf.as_raw_handle(), 0) },
        WAIT_TIMEOUT
    );
    drop(process);
    assert_stopped(&leaf);
}

#[test]
fn forced_owner_exit_closes_the_job_without_running_rust_destructors() {
    let fixture = Fixture::new();
    let mut owner = OwnedProcess::spawn(&mut fixture.command("owner")).unwrap();
    let (root, leaf) = fixture.ready();
    let root = watch(root);
    let leaf = watch(leaf);
    // Kill only the controller process, leaving its child job to kernel cleanup.
    owner.child.kill().unwrap();
    owner.child.wait().unwrap();
    assert_stopped(&root);
    assert_stopped(&leaf);
}

#[test]
fn polling_reaps_workers_when_the_root_exits_but_its_pipes_remain_open() {
    let fixture = Fixture::new();
    let mut process = OwnedProcess::spawn(&mut fixture.command("root")).unwrap();
    let (_, leaf) = fixture.ready();
    let leaf = watch(leaf);
    process.child.kill().unwrap();
    process.child.wait().unwrap();
    process.reap_exited_root();
    assert_stopped(&leaf);
}

#[test]
fn configuration_output_drains_both_pipes_and_retains_unicode() {
    let fixture = Fixture::new();
    let output = OwnedProcess::output(
        fixture
            .command("output")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains(&"配置输出".repeat(32768))
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains(&"诊断输出".repeat(32768))
    );
}

#[test]
#[ignore = "child-process fixture invoked only by the isolated ownership tests"]
fn process_tree_fixture() {
    let Ok(mode) = std::env::var("OMP_GUI_PROCESS_FIXTURE") else {
        return;
    };
    let directory = PathBuf::from(std::env::var_os("OMP_GUI_PROCESS_FIXTURE_DIR").unwrap());
    if mode == "owner" {
        let _root = OwnedProcess::spawn(&mut command("root", &directory)).unwrap();
        thread::sleep(Duration::from_secs(30));
    } else if mode == "root" {
        let mut leaf = command("leaf", &directory).spawn().unwrap();
        fs::write(
            directory.join("ready"),
            format!("{},{}", std::process::id(), leaf.id()),
        )
        .unwrap();
        let _ = leaf.wait();
    } else if mode == "output" {
        print!("{}", "配置输出".repeat(32768));
        eprint!("{}", "诊断输出".repeat(32768));
    } else {
        thread::sleep(Duration::from_secs(30));
    }
}
