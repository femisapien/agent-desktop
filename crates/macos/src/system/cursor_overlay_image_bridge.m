#import "cursor_overlay_chrome.h"
#import <stdbool.h>
#import <stdint.h>
#import <sys/stat.h>

static const off_t ADImageMaxBytes = 2 * 1024 * 1024;
static const uint8_t ADImageSlots = 2;
static const uint8_t ADArrowSlot = 0;
static const uint8_t ADPointerSlot = 1;

@interface ADCursorImageSlot : NSObject
@property(nonatomic, copy) NSString *path;
@property(nonatomic) CGPoint hotspot;
@property(nonatomic, strong) NSImage *loaded;
@property(nonatomic) bool known;
@property(nonatomic) struct timespec modified;
@property(nonatomic) off_t bytes;
@property(nonatomic, strong) id raster;
@property(nonatomic) CGSize rasterPixels;
@end

@implementation ADCursorImageSlot
@end

static __strong NSArray<ADCursorImageSlot *> *ADImageSlotList = nil;
static __strong CALayer *ADImageLayer = nil;
static uint8_t ADImageShown = 0;

static ADCursorImageSlot *ADImageSlot(uint8_t slot) {
    if (ADImageSlotList == nil) {
        NSMutableArray *slots = [NSMutableArray array];
        for (uint8_t index = 0; index < ADImageSlots; index += 1) {
            [slots addObject:[ADCursorImageSlot new]];
        }
        ADImageSlotList = slots;
    }
    return slot < ADImageSlots ? ADImageSlotList[slot] : nil;
}

bool agent_desktop_cursor_overlay_image(uint8_t slot,
                                        const char *path,
                                        double hotspotX,
                                        double hotspotY) {
    @autoreleasepool {
        ADCursorImageSlot *image = ADImageSlot(slot);
        if (image == nil) {
            return false;
        }
        image.path = path == NULL ? nil : [NSString stringWithUTF8String:path];
        image.hotspot = CGPointMake(hotspotX, hotspotY);
        image.loaded = nil;
        image.known = false;
        image.raster = nil;
        image.rasterPixels = CGSizeZero;
        return path == NULL || image.path != nil;
    }
}

static NSImage *ADImageCurrent(ADCursorImageSlot *slot) {
    if (slot.path == nil) {
        return nil;
    }
    struct stat info;
    if (stat(slot.path.fileSystemRepresentation, &info) != 0 || !S_ISREG(info.st_mode) ||
        info.st_size > ADImageMaxBytes) {
        slot.loaded = nil;
        slot.known = false;
        return nil;
    }
    bool unchanged = slot.known && info.st_size == slot.bytes &&
                     info.st_mtimespec.tv_sec == slot.modified.tv_sec &&
                     info.st_mtimespec.tv_nsec == slot.modified.tv_nsec;
    if (unchanged) {
        return slot.loaded;
    }
    NSImage *image = [[NSImage alloc] initWithContentsOfFile:slot.path];
    bool usable = image != nil && image.size.width > 0.0 && image.size.height > 0.0;
    slot.loaded = usable ? image : nil;
    slot.known = true;
    slot.modified = info.st_mtimespec;
    slot.bytes = info.st_size;
    slot.raster = nil;
    slot.rasterPixels = CGSizeZero;
    return slot.loaded;
}

static CALayer *ADImageLayerCreate(void) {
    CALayer *layer = [CALayer layer];
    layer.actions = @{
        @"transform" : NSNull.null,
        @"opacity" : NSNull.null,
        @"position" : NSNull.null,
        @"bounds" : NSNull.null,
        @"hidden" : NSNull.null,
        @"anchorPoint" : NSNull.null,
        @"contents" : NSNull.null,
        @"contentsScale" : NSNull.null,
    };
    return layer;
}

static CGFloat ADImageFit(CGFloat extent, CGFloat room) {
    return extent > room && extent > 0.0 ? room / extent : 1.0;
}

static id ADImageRasterize(ADCursorImageSlot *slot, NSImage *image, CGSize pixels) {
    if (CGSizeEqualToSize(pixels, slot.rasterPixels) && slot.raster != nil) {
        return slot.raster;
    }
    NSBitmapImageRep *raster =
        [[NSBitmapImageRep alloc] initWithBitmapDataPlanes:NULL
                                                pixelsWide:(NSInteger)pixels.width
                                                pixelsHigh:(NSInteger)pixels.height
                                             bitsPerSample:8
                                           samplesPerPixel:4
                                                  hasAlpha:YES
                                                  isPlanar:NO
                                            colorSpaceName:NSDeviceRGBColorSpace
                                               bytesPerRow:0
                                              bitsPerPixel:0];
    NSGraphicsContext *context =
        raster == nil ? nil : [NSGraphicsContext graphicsContextWithBitmapImageRep:raster];
    if (context == nil) {
        return nil;
    }
    [NSGraphicsContext saveGraphicsState];
    NSGraphicsContext.currentContext = context;
    context.imageInterpolation = NSImageInterpolationHigh;
    [image drawInRect:NSMakeRect(0.0, 0.0, pixels.width, pixels.height)
             fromRect:NSZeroRect
            operation:NSCompositingOperationCopy
             fraction:1.0];
    [NSGraphicsContext restoreGraphicsState];
    slot.raster = (__bridge id)raster.CGImage;
    slot.rasterPixels = pixels;
    return slot.raster;
}

static bool ADImageDraw(NSWindow *window, CALayer *pointer, ADCursorImageSlot *slot) {
    NSImage *image = ADImageCurrent(slot);
    CALayer *stage = window.contentView.layer;
    if (image == nil || stage == nil || pointer == nil) {
        return false;
    }
    if (ADImageLayer == nil) {
        ADImageLayer = ADImageLayerCreate();
    }
    if (ADImageLayer.superlayer != stage) {
        [stage addSublayer:ADImageLayer];
    }
    NSSize size = image.size;
    CGFloat hotspotX = MIN(MAX(slot.hotspot.x, 0.0), size.width);
    CGFloat hotspotY = MIN(MAX(slot.hotspot.y, 0.0), size.height);
    CGPoint tip = pointer.position;
    CGSize room = stage.bounds.size;
    CGFloat scale = ADStyle()->size;
    scale *= MIN(MIN(ADImageFit(hotspotX * scale, tip.x),
                     ADImageFit((size.width - hotspotX) * scale, room.width - tip.x)),
                 MIN(ADImageFit(hotspotY * scale, room.height - tip.y),
                     ADImageFit((size.height - hotspotY) * scale, tip.y)));
    CGSize points = CGSizeMake(size.width * scale, size.height * scale);
    CGFloat backing = window.backingScaleFactor > 0.0 ? window.backingScaleFactor : 1.0;
    id contents = ADImageRasterize(
        slot, image, CGSizeMake(ceil(points.width * backing), ceil(points.height * backing)));
    if (contents == nil) {
        return false;
    }
    ADImageLayer.bounds = CGRectMake(0.0, 0.0, points.width, points.height);
    ADImageLayer.anchorPoint = CGPointMake(hotspotX / size.width, 1.0 - hotspotY / size.height);
    ADImageLayer.position = tip;
    ADImageLayer.contents = contents;
    ADImageLayer.contentsScale = backing;
    ADImageLayer.hidden = NO;
    return true;
}

void ADPointerImageApply(NSWindow *window, CALayer *pointer) {
    bool drawn = (ADImageShown == ADPointerSlot &&
                  ADImageDraw(window, pointer, ADImageSlot(ADPointerSlot))) ||
                 ADImageDraw(window, pointer, ADImageSlot(ADArrowSlot));
    if (!drawn) {
        [ADImageLayer removeFromSuperlayer];
    }
    pointer.hidden = drawn;
}

void ADPointerImageSelect(NSWindow *window, CALayer *pointer, bool pointing) {
    uint8_t next = pointing ? ADPointerSlot : ADArrowSlot;
    if (next == ADImageShown) {
        return;
    }
    ADImageShown = next;
    ADPointerImageApply(window, pointer);
}
