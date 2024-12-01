#ifndef THREAD_H
#define THREAD_H

#include "msg.h"
#include "ariel-os-core.h"
#include "cpu_conf.h"
#include "thread_config.h"

typedef void *(*thread_task_func_t)(void *arg);

static inline uint8_t thread_create(char *stack_ptr,
                                    uintptr_t stack_size,
                                    uint8_t priority,
                                    uint32_t flags,
                                    void *(*thread_func)(void *),
                                    void *arg,
                                    const char *_name)
{
    return _thread_create(stack_ptr, stack_size, SCHED_PRIO_LEVELS - 1 - priority, flags,
                          (uintptr_t)thread_func, (uintptr_t)arg, _name);
}

#endif /* THREAD_H */
