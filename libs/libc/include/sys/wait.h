#ifndef _SYS_WAIT_H
#define _SYS_WAIT_H

#include <sys/types.h>

extern pid_t waitpid(pid_t pid, int *status, int options);

#define WNOHANG 1

#endif
