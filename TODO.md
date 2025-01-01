# TODO notes

Because I come back to this project not very frequently I'll use this as a
place to jot down any notes.

## Book Spot

Pdf Page 271 book page number 239.

## Current TODO list 

* Redivert attached process stdout/stderr
* tab autocomplete on commands would be nice
* Command history not being stringly typed (although it only being valid commands and deduped is maybe nice enough)
* Breakpoint setting seems to not work as I expect - something might be wonky
* Go back and do the pipe stuff
* read and write memory commands (chapter 8)
* disassembly (chapter 8)
* hardware breakpoints - for watching variables changing (chapter 9)
* Fix whatever is wrong with my offsetting...
* Better API around siginfo in https://github.com/nix-rust/nix 
* memory mapped ELF files to avoid loading a bunch of big executables when not a lot of them might be parsed

## Diary

### 2025-01-01

Milestone: New years day!

* So I think I solved my elf file woes but we'll see...
* Load and store DWARF sections and ELF file

### 2024-12-31

* So redirecting process output to the pipe isn't working like I'd hoped. I think it's the fork...
* Fix issue where I was waiting on ptrace events in wrong place and made them enable-able (not exposed via command API)
* CTRL+C now stops inferior process
* Trap reason is now stored - and relevant libc PR opened [here](https://github.com/rust-lang/libc/pull/4225) 
    * A nicer API in https://github.com/nix-rust/nix would be nice

### 2024-12-30

* Implement step, status and list-breakpoint commands
* Very simple breakpoint test

### 2024-12-29

Milestone reached Chapter 7 (breakpoints)

* get and set registers implemented
* print command implemented (can print all registers currently)
* Add ID to my breakpoint and hook in commands to set breakpoints
* Store address offset for PIE executables 
* Started jotting something down for single step

### 2024-12-28

Milestone reached Chapter 4 - but decided I didn't need pipes for testing so....
Milestone reached Chapter 5!

* Implement parsing of break and continue commands
* Start on process abstraction
    - Refactor wait logic into process type
    - Expand state variable further
    - Drop impl that kills or detaches
    - Return a stop reason on wait and added `WaitStatus::Signaled` to match
* No longer continue program on launch user has to do first continue

### 2024-12-27

* Created basic layout 
* Added basic command processing - restart or start a new process, show hide debug logs
* Help message
* Command history
* Move from strings to a command enum to make some parts of the code easier!

### 2024-12-26

* Implemented attach
* Started adding in ratatui for a simple UI
* Looking at https://docs.rs/tui-logger for tui logging window
