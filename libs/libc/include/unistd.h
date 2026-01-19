#ifndef _UNISTD_H
#define _UNISTD_H

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

#define R_OK 4
#define W_OK 2
#define X_OK 1
#define F_OK 0

int close(int fd);
ssize_t read(int fd, void *buf, size_t count);
ssize_t write(int fd, const void *buf, size_t count);
int access(const char *pathname, int mode);
int isatty(int fd);
pid_t getpid(void);
int unlink(const char *pathname);
int gethostname(char *name, size_t len);
int fsync(int fd);
int fchown(int fd, uid_t owner, gid_t group);
int fchmod(int fd, mode_t mode);
int chmod(const char *path, mode_t mode);
unsigned int sleep(unsigned int seconds);
int usleep(unsigned int usec);
uid_t getuid(void);
uid_t geteuid(void);
int chdir(const char *path);
char *getcwd(char *buf, size_t size);

#ifdef __cplusplus
}
#endif

#endif