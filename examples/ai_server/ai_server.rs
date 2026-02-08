/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! This example showcases how you can use the "AI Server" deployment style for Cranium. 
//! 
//! While we're using Rust for convenience, the AI Server API is entirely a native dynamic library 
//! (i.e. a DLL on Windows, an SO for Linux, etc.). You should be able to use it with any language 
//! or runtime - as long as you can call arbitrary DLLs (or equivalents) from it.
//! 
//! As a quick recap - "AI Server" means that you run a separate dedicated Bevy app for Cranium. 
//! 
//! This app is entirely self-contained and isolated; it is purely a sandbox for the Cranium AI engine. 
//! This means it has no access to or knowledge of the context that you're using it in - unless you provide it! 
//! 
//! All it does is sit around, wait for AI decision requests, and when one comes - use the world-state you've 
//! updated it with most recently to calculate the best course of action.
//! 
//! I'm not selling it very well, huh?
//! 
//! Except all this also means you can run Cranium does not *require* access to your app's guts! 
//! 
//! As long as you have a way of updating Cranium about the relevant bits of state of your application 
//! via the exposed `cranium_api` methods, you can integrate Cranium's AI engine *anywhere*. 
//! 
//! A Python roguelike? A game in a Lua-based engine? A C++ desktop app? A custom Rust game engine? Embedded? 
//! 
//! Anything goes (as long as you can talk to the API in your environment)!

// We'll use a thread to run Cranium in the background.
use std::{thread};
use std::{time::Duration};

// We'll reuse the types from the Rust library to avoid redeclaring; the example already needed to build 
// cranium_api because of how Cargo examples work, but in a real example we don't quite need this.
use cranium_ffi::{ApiInMsg, ApiOutMsg, FFIOption};


// Bindings to the relevant functions in the DLL.
// Note that we are NOT actually calling cranium_api's 'native' Rust methods here, we're calling the DLL!
#[cfg_attr(target_os = "windows", link(name = "target/debug/deps/cranium_api.dll", kind = "dylib"))]
#[cfg_attr(target_os = "linux", link(name = "libcranium_api.so", kind = "dylib"))]
#[cfg_attr(target_os = "android", link(name = "libcranium_api.so", kind = "dylib"))]
#[cfg_attr(target_os = "macos", link(name = "target/debug/deps/cranium_api.dylib", kind = "dylib"))]
unsafe extern "C" {
    safe fn cranium_create_and_autorun();
    safe fn cranium_keepalive();
    safe fn cranium_await_message() -> FFIOption<ApiOutMsg>;
    safe fn cranium_try_get_message() -> FFIOption<ApiOutMsg>;
    safe fn cranium_write_ping() -> bool;
}

/// A trivial Rusty wrapper for the extern function to make thread::spawn happy.
fn create_and_autorun() {
    cranium_create_and_autorun();
}

/// Creates a Cranium server in the background on a worker thread.
fn spawn_cranium_server() -> thread::JoinHandle<()> {
    thread::spawn(create_and_autorun)
}

/// The main showcase. 
/// 
/// We won't be doing anything serious here as this example is mainly meant to showcase the setup required. 
/// We'll create a Cranium AI Server and do a bit of back-and-forth communication with it, then we'll let 
/// it gracefully shut down using its heartbeat-timeout functionality.
/// 
/// We'll expect the whole thing to run for 3 minutes (assuming the CORTEX_AUTORUN_HEARTBEAT_TIMEOUT_SECONDS
/// envvar has not been set to a custom value) - we'll send a keepalive at 1 min mark and expect the default 
/// timeout of 2 mins after that point.
fn main() {
    let start_time = std::time::Instant::now();
    println!("Starting server at {:?}", start_time);
    let cranium_thread = spawn_cranium_server();
    let mut ctr = 0u8;
    let mut got_start = false;
    
    while !cranium_thread.is_finished() && !got_start {
        got_start = cranium_await_message().is_some();
    }

    while !cranium_thread.is_finished() {
        thread::sleep(Duration::from_secs(5));
        println!("Slept for 5s, now at {:?}", start_time.elapsed());
        let wrote_ping = cranium_write_ping();
        if wrote_ping {
            println!("Sent a ping");
        }

        let maybe_msg: Option<ApiOutMsg> = cranium_try_get_message().into();
        if let Some(msg) = maybe_msg {
            println!("Received a message: {:?}", msg);
        }
        ctr += 1;
        if ctr == 12 || ctr == 24 {
            println!("Sending a keep-alive heartbeat...");
            cranium_keepalive();
        }
    }
}
