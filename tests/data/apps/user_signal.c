#include "stdio.h"
#include "stdlib.h"
#include "signal.h"
#include "unistd.h"

// Signal handler function
void handle_signal(int sig) {
    if (sig == SIGUSR1) {
        printf("\nReceived SIGUSR1. Exiting gracefully...\n");
        exit(0);
    }
}

int main() {
    // Register the signal handler for SIGINT (Ctrl+C)
    if (signal(SIGUSR1, handle_signal) == SIG_ERR) {
        printf("Error registering signal handler.\n");
        return 1;
    }

    // Emitting SIGUSR to the current process after 1 seconds
    printf("Program will emit SIGINT to itself in 1 seconds...\n");
    sleep(1);

    // Sending SIGINT to this process (this will invoke handle_signal)
    if (kill(getpid(), SIGUSR1) == -1) {
        perror("Error sending SIGUSR1");
        return 1;
    }

    // Infinite loop to keep the program running
    while (1) {
        printf("Running... Waiting for our signal\n");
        sleep(1);  // Sleep for 1 second
    }

    return 0;
}
