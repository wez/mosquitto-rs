#pragma once
// This is a slightly janky zero-dependency pthread compatibility
// header that seems to have just enough guff for the mosquitto
// client to run on Windows

typedef HANDLE pthread_t;

typedef struct posix_thread_attr {
  int dummy;
} pthread_attr_t;

typedef HANDLE pthread_mutex_t;

typedef struct posix_mutex_attr {
  int dummy;
} pthread_mutexattr_t;

static inline pthread_t pthread_self(void) {
  return GetCurrentThread();
}

static inline int pthread_equal(pthread_t t1, pthread_t t2) {
  return t1 == t2;
}

struct pthread_create_trampoline_info {
  void *(*start_routine)(void*);
  void *arg;
  HANDLE event;
};

static inline DWORD WINAPI pthread_create_trampoline(LPVOID p) {
  struct pthread_create_trampoline_info info = *(struct pthread_create_trampoline_info*)p;
  // Signal creator that we have copied the arguments
  SetEvent(info.event);
  return (DWORD)info.start_routine(info.arg);
}

static inline int pthread_create(pthread_t *thread,
    const pthread_attr_t *attr,
    void *(*start_routine) (void *),
    void *arg) {
  struct pthread_create_trampoline_info info = {
    start_routine,
    arg,
    CreateEventA(NULL, FALSE, FALSE, NULL),
  };
  *thread = CreateThread(NULL, 0, pthread_create_trampoline, &info,0, NULL);
  WaitForSingleObject(info.event, INFINITE);
  CloseHandle(info.event);
  if (*thread) {
    return 0;
  }
  return ENOMEM;
}

static inline int pthread_join(pthread_t thread, void **retval) {
  HANDLE me = GetCurrentThread();
  if (thread == me) {
    // Can't join myself
    return EDEADLK;
  }

  WaitForSingleObject(thread, INFINITE);
  if (retval) {
    DWORD status;
    GetExitCodeThread(thread, &status);
    *retval = (void*)status;
  }

  CloseHandle(thread);

  return 0;
}

#ifdef HAVE_PTHREAD_CANCEL
static inline int pthread_cancel(pthread_t thread) {
  // There is no co-operative cancellation
  return ENOSYS;
}

static inline void pthread_testcancel(void) {
  // There is no co-operative cancellation
}
#endif

static inline int pthread_mutex_destroy(pthread_mutex_t *mutex) {
  CloseHandle(mutex);
  return 0;
}

static inline int pthread_mutex_init(pthread_mutex_t *restrict mutex,
const pthread_mutexattr_t *restrict attr) {
  *mutex = CreateMutexA(NULL, FALSE, NULL);
  if (*mutex) {
    return 0;
  }
  return ENOMEM;
}

static inline int pthread_mutex_lock(pthread_mutex_t *mutex) {
  if (WaitForSingleObject(mutex, INFINITE) == WAIT_OBJECT_0) {
    return 0;
  }
  return EINVAL;
}

static inline int pthread_mutex_unlock(pthread_mutex_t *mutex) {
  ReleaseMutex(mutex);
  return 0;
}
