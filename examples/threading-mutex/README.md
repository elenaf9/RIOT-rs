# Threading Mutex

## About

This application demonstrates how multiple threads can wait for the same mutex.
The waiting threads get unblocked by priority, and within a priority in FIFO order.
The current mutex owner inherits the priority of the highest priority waiting thread.

## How to run

In this folder, run

    laze build -b nrf52840dk run

The application will start an async task that acquires a mutex and holds it for a couple of seconds, and three threads with different priorities that wait for the same mutex.
