# Example - AI Server

## What's this?

This example showcases how you can use the "AI Server" deployment style for Cranium. 

While we're using Rust for convenience, the AI Server API is entirely a native dynamic library 
(i.e. a DLL on Windows, an SO for Linux, etc.). You should be able to use it with any language 
or runtime - as long as you can call arbitrary DLLs (or equivalents) from it.

## Why bother?

As a quick recap - "AI Server" means that you run a separate dedicated Bevy app for Cranium. 

This app is entirely self-contained and isolated; it is purely a sandbox for the Cranium AI engine. 
This means it has no access to or knowledge of the context that you're using it in - unless you provide it! 

All it does is sit around, wait for AI decision requests, and when one comes - use the world-state you've 
updated it with most recently to calculate the best course of action.

I'm not selling it very well, huh?

Except all this also means Cranium does not *require* access to your app's guts to operate! 

As long as you have a way of updating Cranium about the relevant bits of state of your application 
via the exposed `cranium_api` methods, you can integrate Cranium's AI engine *anywhere*. 

A Python roguelike? A game in a Lua-based engine? A C++ desktop app? A custom Rust game engine? Embedded? 

Anything goes (as long as you can talk to the API in your environment)!


## The Scenario

We won't be doing anything serious here as this example is mainly meant to showcase the setup required. 

We'll create a Cranium AI Server and do a bit of back-and-forth communication with it, 
then we'll let it gracefully shut down using its heartbeat-timeout functionality.
