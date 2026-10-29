// Scripts/shot.m — the capture rig the screenshot tool injects.
//
// Scripts/shot.sh compiles this file into a dylib, loads it into the running
// Tandem app with dlopen (via lldb), and calls tandem_shot(windowNumber, path).
// The app renders its own window in process with cacheDisplay — `screencapture`
// has no Screen Recording permission on this Mac, so the app photographs
// itself instead (docs/screenshots/README.md).
//
// This file is camera mechanics only. It checks nothing about homes: the gate
// is Scripts/shot.sh, which refuses any TANDEM_HOME that is not the repo's
// fixture home before this code is ever loaded.

#import <Cocoa/Cocoa.h>

// Render one window of this process to a PNG. Runs on the app's main thread —
// lldb evaluates on thread 1, which is the app's main thread for a GUI app.
int tandem_shot(long windowNumber, const char *pngPath) {
  NSWindow *window = nil;
  for (NSWindow *w in NSApp.windows) {
    if (w.windowNumber == windowNumber) {
      window = w;
      break;
    }
  }
  if (!window) {
    fprintf(stderr, "tandem_shot: no window numbered %ld in this process\n", windowNumber);
    return 1;
  }

  // The window's content view is what cacheDisplay renders — the method is
  // declared on NSView, not NSWindow, and the content view's bounds (not the
  // window frame's) are the rect the pixels cover.
  NSView *view = window.contentView;
  NSSize bounds = view.bounds.size;
  double scale = window.backingScaleFactor;
  NSBitmapImageRep *rep =
      [[NSBitmapImageRep alloc] initWithBitmapDataPlanes:NULL
                                              pixelsWide:(NSInteger)round(bounds.width * scale)
                                              pixelsHigh:(NSInteger)round(bounds.height * scale)
                                           bitsPerSample:8
                                         samplesPerPixel:4
                                                hasAlpha:YES
                                                 isPlanar:NO
                                           colorSpaceName:NSCalibratedRGBColorSpace
                                              bytesPerRow:0
                                             bitsPerPixel:0];
  if (!rep) {
    fprintf(stderr, "tandem_shot: bitmap rep failed\n");
    return 1;
  }

  // The rep's size in points is the view's bounds, not its pixel count: without
  // this line the rep is `bounds * scale` points wide, and cacheDisplay draws the
  // view at one point per pixel into the bottom-left quarter of the bitmap.
  rep.size = bounds;

  [view cacheDisplayInRect:view.bounds toBitmapImageRep:rep];
  NSData *png = [rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
  if (!png || ![png writeToFile:@(pngPath) atomically:YES]) {
    fprintf(stderr, "tandem_shot: could not write %s\n", pngPath);
    return 1;
  }
  return 0;
}
