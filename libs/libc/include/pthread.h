#ifndef _PTHREAD_H
#define _PTHREAD_H

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef unsigned long pthread_t;
typedef void* pthread_attr_t;

int pthread_create(pthread_t *thread, const pthread_attr_t *attr,
                   void *(*start_routine) (void *), void *arg);
int pthread_join(pthread_t thread, void **retval);
void pthread_exit(void *retval);
pthread_t pthread_self(void);

#ifdef __cplusplus
}
#endif

#endif
