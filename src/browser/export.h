#ifndef ABROWSER_SRC_BROWSER_EXPORT_H_
#define ABROWSER_SRC_BROWSER_EXPORT_H_

// ABROWSER_BRIDGE_EXPORT
#if defined(COMPONENT_BUILD)

#if defined(WIN32)

#if defined(ABROWSER_BRIDGE_IMPLEMENTATION)
#define ABROWSER_BRIDGE_EXPORT __declspec(dllexport)
#else
#define ABROWSER_BRIDGE_EXPORT __declspec(dllimport)
#endif

#else  // !defined(WIN32)

#if defined(ABROWSER_BRIDGE_IMPLEMENTATION)
#define ABROWSER_BRIDGE_EXPORT __attribute__((visibility("default")))
#else
#define ABROWSER_BRIDGE_EXPORT
#endif

#endif

#else  // !defined(COMPONENT_BUILD)

#define ABROWSER_BRIDGE_EXPORT

#endif

// ABROWSER_RENDERER_EXPORT
#if defined(COMPONENT_BUILD)

#if defined(WIN32)

#if defined(ABROWSER_RENDERER_IMPLEMENTATION)
#define ABROWSER_RENDERER_EXPORT __declspec(dllexport)
#else
#define ABROWSER_RENDERER_EXPORT __declspec(dllimport)
#endif

#else  // !defined(WIN32)

#if defined(ABROWSER_RENDERER_IMPLEMENTATION)
#define ABROWSER_RENDERER_EXPORT __attribute__((visibility("default")))
#else
#define ABROWSER_RENDERER_EXPORT
#endif

#endif

#else  // !defined(COMPONENT_BUILD)

#define ABROWSER_RENDERER_EXPORT

#endif

#endif  // ABROWSER_SRC_BROWSER_EXPORT_H_
