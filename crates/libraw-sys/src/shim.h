#ifndef CLIBRAW_SHIM_H
#define CLIBRAW_SHIM_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle to a libraw_data_t. */
typedef void *awpr_raw;

/* ---- lifecycle ---------------------------------------------------------- */

int          awpr_libraw_available(void);
const char  *awpr_libraw_version(void);

awpr_raw     awpr_open(const char *path);          /* NULL on failure */
void         awpr_close(awpr_raw h);

/* ---- sizes -------------------------------------------------------------- */

/* Mirrors the fields the Windows build read by byte offset out of
   libraw_image_sizes_t. Here they come from the real struct. */
typedef struct {
    int raw_width, raw_height;
    int width, height;          /* libraw's idea of the visible area */
    int left_margin, top_margin;
    int iwidth, iheight;
    int flip;                   /* 0 none, 3 = 180, 5 = ccw90, 6 = cw90 */
} awpr_sizes;

/* Snapshot after open, then again after unpack: some formats only fix up the
   visible area during unpack, but a failed unpack recycles (zeroes) the struct,
   so the post-open snapshot is kept as the fallback. */
int awpr_read_sizes(const char *path, awpr_sizes *out);   /* 1 = ok */

/* ---- camera colour data ------------------------------------------------- */

/* pre_mul[3], cam_mul[3], rgb_cam[3][3] (row-major, 9 doubles).
   Returns 0 when the file cannot be opened or the sensor has a 4th colour
   channel (rgb_cam column 3 non-zero) — the caller then falls back to the
   black-body approximation, exactly as the Windows build does. */
int awpr_read_camera_color(const char *path, double *pre_mul, double *cam_mul, double *rgb_cam);

/* ---- metadata ----------------------------------------------------------- */

/* What the info panel shows, straight from LibRaw's own parse (Rust port addition:
   the Windows build asks ExifTool and the Swift build ImageIO; LibRaw reads the same
   tags on every platform). Strings are NUL-terminated UTF-8 / ASCII. */
typedef struct {
    char make[64];
    char model[64];
    char lens[128];
    float iso_speed;
    float shutter;               /* seconds */
    float aperture;              /* f-number */
    float focal_len;             /* mm */
    long long timestamp;         /* time_t of DateTimeOriginal, 0 = unknown */
    int width, height;           /* visible size after the camera's flip */
    int flip;
} awpr_meta;

int awpr_read_meta(const char *path, awpr_meta *out);   /* 1 = ok */

/* ---- decode ------------------------------------------------------------- */

/* A decoded image handed back to Swift. `data` stays owned by libraw until
   awpr_free_image is called. */
typedef struct {
    void *opaque;               /* libraw_processed_image_t*        */
    awpr_raw owner;             /* the libraw handle to close later */
    const void *data;           /* interleaved samples              */
    int type;                   /* 1 = JPEG blob, 2 = bitmap        */
    int width, height, colors, bits;
    int data_size;
} awpr_image;

/* Full-resolution demosaiced decode. bps is 8 or 16.
   Matches the Windows settings: output_color = 1 (sRGB), no_auto_bright = 0. */
int awpr_decode_full(const char *path, int bps, awpr_image *out);

/* The camera's embedded preview (fast). May come back as a JPEG blob (type 1). */
int awpr_decode_thumb(const char *path, awpr_image *out, int *flip);

void awpr_free_image(awpr_image *img);

#ifdef __cplusplus
}
#endif
#endif
