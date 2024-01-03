use sysinfo::{
	Pid,
	ProcessExt, 
	System, 
	SystemExt
};

pub fn is_running(pid: usize) -> bool {
	let system = System::new();
	
	system.process(Pid::from(pid)).is_some()
}

pub fn kill(pid: usize) {
	let system = System::new();
	let proc = system.process(Pid::from(pid)).expect("Expected a running process.");

	proc.kill();
}

pub fn get_name(pid: usize) -> Option<String> {
	let system = System::new();
	let proc = system.process(Pid::from(pid))?;

	Some(proc.name().to_string())
}
