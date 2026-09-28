#import "../../crates/macos/src/system/cursor_overlay_chrome_bridge.m"
#import "../../crates/macos/src/system/cursor_overlay_image_bridge.m"
#import <unistd.h>

static void require(bool condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "%s\n", message);
        exit(1);
    }
}

static NSString *writePNG(NSString *name, NSInteger width, NSInteger height) {
    NSBitmapImageRep *rep = [[NSBitmapImageRep alloc] initWithBitmapDataPlanes:NULL
                                                                    pixelsWide:width
                                                                    pixelsHigh:height
                                                                 bitsPerSample:8
                                                               samplesPerPixel:4
                                                                      hasAlpha:YES
                                                                      isPlanar:NO
                                                                colorSpaceName:NSDeviceRGBColorSpace
                                                                   bytesPerRow:0
                                                                  bitsPerPixel:0];
    NSString *path = [NSTemporaryDirectory()
        stringByAppendingPathComponent:[NSString stringWithFormat:@"%@-%d.png", name, getpid()]];
    NSData *data = [rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
    require([data writeToFile:path atomically:YES], "fixture image must be written");
    return path;
}

static bool near(CGFloat left, CGFloat right) {
    return fabs(left - right) < 0.001;
}

int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSWindow *window = ADWindow(NSMakeRect(0.0, 0.0, 240.0, 240.0));
        CALayer *stage = window.contentView.layer;
        CALayer *pointer = [CALayer layer];
        pointer.position = CGPointMake(88.0, 172.0);
        [stage addSublayer:pointer];

        NSString *small = writePNG(@"ad-cursor-small", 20, 30);
        require(agent_desktop_cursor_overlay_image(0, small.fileSystemRepresentation, 4.0, 2.0),
                "image path must be accepted");
        ADPointerImageApply(window, pointer);
        require(ADImageLayer.superlayer == stage, "image must be a sibling of the pointer");
        require(pointer.sublayers.count == 0, "image must not join the arrow's sublayers");
        require(pointer.hidden && !ADImageLayer.hidden, "image must replace the arrow");
        require(near(ADImageLayer.bounds.size.width, 20.0) &&
                    near(ADImageLayer.bounds.size.height, 30.0),
                "image must keep its point size at size 1");
        require(near(ADImageLayer.anchorPoint.x, 4.0 / 20.0) &&
                    near(ADImageLayer.anchorPoint.y, 1.0 - 2.0 / 30.0),
                "hotspot must anchor the image from its top-left");
        require(CGPointEqualToPoint(ADImageLayer.position, pointer.position),
                "hotspot must sit on the cursor tip");
        require(ADImageLayer.contents != nil, "image must be rasterized");

        AgentDesktopCursorStyle style = *ADStyle();
        style.size = 4.0;
        agent_desktop_cursor_overlay_style(&style);
        NSString *large = writePNG(@"ad-cursor-large", 200, 200);
        agent_desktop_cursor_overlay_image(0, large.fileSystemRepresentation, 0.0, 0.0);
        ADPointerImageApply(window, pointer);
        CGSize fitted = ADImageLayer.bounds.size;
        require(fitted.width <= 152.0 + 0.001 && fitted.height <= 172.0 + 0.001,
                "oversized image must be scaled down to the stage");
        require(near(fitted.width, fitted.height), "downscale must be uniform");

        [[NSFileManager defaultManager] removeItemAtPath:large error:nil];
        ADPointerImageApply(window, pointer);
        require(ADImageLayer.superlayer == nil && !pointer.hidden,
                "a missing image must fall back to the arrow");

        agent_desktop_cursor_overlay_image(0, small.fileSystemRepresentation, 0.0, 0.0);
        ADPointerImageApply(window, pointer);
        require(pointer.hidden, "a restored image must be drawn again");
        agent_desktop_cursor_overlay_image(0, NULL, 0.0, 0.0);
        ADPointerImageApply(window, pointer);
        require(ADImageLayer.superlayer == nil && !pointer.hidden,
                "clearing the image must restore the arrow");

        style.size = 1.0;
        agent_desktop_cursor_overlay_style(&style);
        NSString *hand = writePNG(@"ad-cursor-hand", 26, 28);
        agent_desktop_cursor_overlay_image(1, hand.fileSystemRepresentation, 8.0, 0.0);
        ADPointerImageApply(window, pointer);
        require(!pointer.hidden && ADImageLayer.superlayer == nil,
                "a pointer image alone must leave the arrow until arrival");
        ADPointerImageSelect(window, pointer, true);
        require(pointer.hidden && near(ADImageLayer.bounds.size.width, 26.0) &&
                    near(ADImageLayer.anchorPoint.x, 8.0 / 26.0),
                "arrival on a control must show the pointer image at its hotspot");
        agent_desktop_cursor_overlay_image(0, small.fileSystemRepresentation, 4.0, 2.0);
        ADPointerImageSelect(window, pointer, false);
        require(pointer.hidden && near(ADImageLayer.bounds.size.width, 20.0),
                "departure must switch back to the arrow image");
        agent_desktop_cursor_overlay_image(0, NULL, 0.0, 0.0);
        ADPointerImageApply(window, pointer);
        require(!pointer.hidden && ADImageLayer.superlayer == nil,
                "departure without an arrow image must show the built-in arrow");
        ADPointerImageSelect(window, pointer, true);
        [[NSFileManager defaultManager] removeItemAtPath:hand error:nil];
        ADPointerImageApply(window, pointer);
        require(!pointer.hidden, "a deleted pointer image must fall back to the arrow");
        [[NSFileManager defaultManager] removeItemAtPath:small error:nil];
    }
    return 0;
}
