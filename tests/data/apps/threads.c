#include "stdio.h"
#include "stdlib.h"
#include "pthread.h"
#include "unistd.h"

// Function that each thread will execute
void* thread_function(void* arg) {
    int thread_id = *((int*)arg);
    printf("Thread %d is starting.\n", thread_id);
    // Simulate some work
    sleep(1);
    printf("Thread %d is done.\n", thread_id);
    return NULL;
}

int main() {
    const int num_threads = 5;  // Number of threads to create
    pthread_t threads[num_threads];  // Array to hold thread identifiers
    int thread_ids[num_threads];    // Array to hold thread IDs (arguments to pass to threads)

    // Create threads
    for (int i = 0; i < num_threads; ++i) {
        thread_ids[i] = i;  // Set the thread ID
        if (pthread_create(&threads[i], NULL, thread_function, &thread_ids[i]) != 0) {
            perror("Failed to create thread");
            return 1;
        }
    }

    // Wait for all threads to finish
    for (int i = 0; i < num_threads; ++i) {
        if (pthread_join(threads[i], NULL) != 0) {
            perror("Failed to join thread");
            return 1;
        }
    }

    printf("All threads have finished.\n");
    return 0;
}

