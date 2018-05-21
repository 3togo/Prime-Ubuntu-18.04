extern crate tokio_uds;
extern crate futures;
extern crate tokio_core;

use tokio_uds::{UnixListener, UnixStream};
use futures::{Stream, Future};
use tokio_core::reactor::Core;
use std::io::Read;
use std::fs;
use std::fs::File;
use std::process::Command;
use std::time::Duration;
use std::thread::sleep;
use std::io::{Error, ErrorKind};
use std::cell::{RefCell, RefMut};

enum GPUState {
    Nvidia,
    Intel
}

fn get_curr_state() -> Result<GPUState, Error> {
    let power_profile_path = "/etc/prime-discrete";
    let mut curr_str = String::new();
    let fd = File::open(power_profile_path)?.read_to_string(&mut curr_str)?;
    eprintln!("{}", curr_str.trim());
    match curr_str.trim() {
        "off" => Ok(GPUState::Intel),
        "on" => Ok(GPUState::Nvidia),
        _ => Err(Error::new(ErrorKind::Other, "unexpected file content"))
    }
}

fn main() {
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let addr = "/tmp/prime_sock";
    let ctl = match UnixListener::bind(addr, &handle) {
        Ok(l) => l,
        Err(_) => {
            fs::remove_file(addr).unwrap();
            UnixListener::bind(addr, &handle).unwrap()
        }
    };

    let tk = ctl.incoming()
        .for_each(|(_, _)| {
            let curr_state = get_curr_state()?;

            let kill_x = Command::new("sh")
                .arg("-c")
                .arg("systemctl disable lightdm && systemctl stop lightdm")
                .status()?;

            if !kill_x.success() {
                return Err(Error::new(ErrorKind::Other, "Could not stop lightdm"));
            }

            match curr_state {
                GPUState::Intel => {
                    sleep(Duration::from_millis(500));
                    let load_mods = Command::new("sh")
                        .arg("-c")
                        .arg("modprobe nvidia")
                        .status()?;

                    if !load_mods.success() {
                        return Err(Error::new(ErrorKind::Other, "Could not load nvidia module"));
                    }
                },
                GPUState::Nvidia => {
                    sleep(Duration::from_millis(500));
                    let unload_mods = Command::new("sh")
                        .arg("-c")
                        .arg("rmmod nvidia-drm && rmmod nvidia-uvm && rmmod nvidia-modeset && rmmod nvidia && modprobe bbswitch && echo OFF > /proc/acpi/bbswitch")
                        .status()?;

                    if !unload_mods.success() {
                        return Err(Error::new(ErrorKind::Other, "Could not unload nvidia modules"));
                    }
                }
            }

            sleep(Duration::from_millis(500));
            let restart_x = Command::new("sh")
                .arg("-c")
                .arg("dpkg-reconfigure lightdm && systemctl start lightdm")
                .status()?;

            if !restart_x.success() {
                return Err(Error::new(ErrorKind::Other, "Could not restart lightdm"));
            }
            Ok(())
        })
        .map_err(|e| {
            eprintln!("Error = {}", e);
        });

    core.run(tk);
}