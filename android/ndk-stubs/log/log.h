/* fdk-aac include "log/log.h" quando è definito ANDROID (è un'intestazione di AOSP, non dell'NDK): qui le sue
 * macro di log (ALOGE per le sole verifiche interne) non servono, quindi sono vuote. */
#ifndef CEDRO_STUB_LOG_H
#define CEDRO_STUB_LOG_H
#define android_errorWriteLog(tag, subTag) ((void)0)
#define ALOGE(...) ((void)0)
#define ALOGW(...) ((void)0)
#define ALOGI(...) ((void)0)
#define ALOGD(...) ((void)0)
#define ALOGV(...) ((void)0)
#endif
