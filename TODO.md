# TODO notes

Because I come back to this project not very frequently I'll use this as a
place to jot down any notes.

## Book Spot

Pdf Page 102 book page number 70.

## Current TODO list 

* Redivert attached process stdout/stderr
* Breakpoint setting etc
* tab autocomplete on commands would be nice
* Command history not being stringly typed (although it only being valid commands and deduped is maybe nice enough)

## Diary

### 2024-12-28

Milestone reached Chapter 4 - but decided I didn't need pipes for testing so....
Milestone reached Chatper 5!

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
