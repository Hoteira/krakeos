#ifndef _UNISTD_H
#define _UNISTD_H

#include <sys/types.h>

extern int usleep(unsigned int usec);
extern ssize_t read(int fd, void *buf, size_t count);
extern ssize_t write(int fd, const void *buf, size_t count);
extern int close(int fd);
extern off_t lseek(int fd, off_t offset, int whence);
extern int isatty(int fd);
extern int access(const char *pathname, int mode);
extern int gethostname(char *name, size_t len);
extern char *getcwd(char *buf, size_t size);
extern pid_t getpid(void);
extern uid_t geteuid(void);

#define F_OK 0
#define X_OK 1
#define W_OK 2
#define R_OK 4

#endif
