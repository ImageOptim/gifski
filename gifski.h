#include <stdint.h>
#include <stdlib.h>
#include <stdbool.h>

struct gifski;
typedef struct gifski gifski;

/*
 * Settings for creating a new encoder instance. See `gifski_new`
 */
typedef struct {
  /*
   * Resize to max this width if non-0
   */
  uint32_t width;
  /*
   * Resize to max this height if width is non-0. Note that aspect ratio is not preserved.
   */
  uint32_t height;
  /*
   * 1-100. Recommended to set to 100.
   */
  uint8_t quality;
  /*
   * If true, looping is disabled
   */
  bool once;
  /*
   * Lower quality, but faster encode
   */
  bool fast;
} GifskiSettings;

/*
 * Call to start the process
 *
 * See `gifski_add_frame_png_file` and `gifski_end_adding_frames`
 */
gifski *gifski_new(const GifskiSettings *settings);

/*
 * File path must be valid UTF-8. This function is asynchronous.
 *
 * Delay is in 1/100ths of a second
 *
 * Call `gifski_end_adding_frames()` after you add all frames. See also `gifski_write()`
 */
bool gifski_add_frame_png_file(gifski *handle,
                               uint32_t index,
                               const char *file_path,
                               uint16_t delay);

/*
 * Pixels is an array width×height×4 bytes large. The array is copied, so you can free/reuse it immediately.
 *
 * Delay is in 1/100ths of a second
 *
 * Call `gifski_end_adding_frames()` after you add all frames. See also `gifski_write()`
 */
bool gifski_add_frame_rgba(gifski *handle,
                           uint32_t index,
                           uint32_t width,
                           uint32_t height,
                           const unsigned char *pixels,
                           uint16_t delay);

/*
 * You must call it at some point (after all frames are set), otherwise `gifski_write()` will never end!
 */
bool gifski_end_adding_frames(gifski *handle);

/*
 * Write frames to `destination` and keep waiting for more frames until `gifski_end_adding_frames` is called.
 */
bool gifski_write(gifski *handle,
                  const char *destination);

/*
 * Call to free all memory
 */
void gifski_drop(gifski *g);
