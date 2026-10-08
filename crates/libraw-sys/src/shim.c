#include "shim.h"

#include <libraw/libraw.h>
#include <string.h>
#include <math.h>

/* LIBRAW_IMAGE_JPEG = 1, LIBRAW_IMAGE_BITMAP = 2 */

/* Paths arrive as UTF-8 from Rust. On Windows libraw_open_file would read them as the
   ANSI code page, so a photo in a folder named in Chinese would fail to open; convert to
   UTF-16 and use the wide entry point instead. (Rust port addition — the Swift build
   never runs on Windows.) */
#ifdef _WIN32
#include <windows.h>
#include <stdlib.h>
static int open_utf8(libraw_data_t *lr, const char *path)
{
    int n = MultiByteToWideChar(CP_UTF8, 0, path, -1, NULL, 0);
    if (n <= 0) return LIBRAW_IO_ERROR;
    wchar_t *w = (wchar_t *)malloc(sizeof(wchar_t) * (size_t)n);
    if (!w) return LIBRAW_UNSUFFICIENT_MEMORY;
    MultiByteToWideChar(CP_UTF8, 0, path, -1, w, n);
    int rc = libraw_open_wfile(lr, w);
    free(w);
    return rc;
}
#else
static int open_utf8(libraw_data_t *lr, const char *path) { return libraw_open_file(lr, path); }
#endif

int awpr_libraw_available(void)
{
    libraw_data_t *lr = libraw_init(0);
    if (!lr) return 0;
    libraw_close(lr);
    return 1;
}

const char *awpr_libraw_version(void) { return libraw_version(); }

int awpr_camera_count(void) { return libraw_cameraCount(); }

const char *awpr_camera_name(int index)
{
    const char **list = libraw_cameraList();
    return (list && index >= 0 && index < libraw_cameraCount()) ? list[index] : 0;
}

awpr_raw awpr_open(const char *path)
{
    libraw_data_t *lr = libraw_init(0);
    if (!lr) return NULL;
    if (open_utf8(lr, path) != LIBRAW_SUCCESS) { libraw_close(lr); return NULL; }
    return (awpr_raw)lr;
}

void awpr_close(awpr_raw h) { if (h) libraw_close((libraw_data_t *)h); }

/* ---- sizes -------------------------------------------------------------- */

static void snapshot_sizes(libraw_data_t *lr, awpr_sizes *s)
{
    s->raw_width   = lr->sizes.raw_width;
    s->raw_height  = lr->sizes.raw_height;
    s->width       = lr->sizes.width;
    s->height      = lr->sizes.height;
    s->left_margin = lr->sizes.left_margin;
    s->top_margin  = lr->sizes.top_margin;
    s->iwidth      = lr->sizes.iwidth;
    s->iheight     = lr->sizes.iheight;
    s->flip        = lr->sizes.flip;
}

int awpr_read_sizes(const char *path, awpr_sizes *out)
{
    if (!out) return 0;
    libraw_data_t *lr = libraw_init(0);
    if (!lr) return 0;
    if (open_utf8(lr, path) != LIBRAW_SUCCESS) { libraw_close(lr); return 0; }

    awpr_sizes after_open;
    snapshot_sizes(lr, &after_open);

    libraw_unpack(lr);                 /* return value deliberately ignored */
    awpr_sizes after_unpack;
    snapshot_sizes(lr, &after_unpack);

    *out = (after_unpack.raw_width > 0 && after_unpack.raw_height > 0)
         ? after_unpack : after_open;
    libraw_close(lr);
    return 1;
}

/* ---- camera colour ------------------------------------------------------ */

int awpr_read_camera_color(const char *path, double *pre_mul, double *cam_mul, double *rgb_cam)
{
    libraw_data_t *lr = libraw_init(0);
    if (!lr) return 0;
    if (open_utf8(lr, path) != LIBRAW_SUCCESS) { libraw_close(lr); return 0; }

    for (int i = 0; i < 3; i++) {
        pre_mul[i] = (double)lr->color.pre_mul[i];
        cam_mul[i] = (double)lr->color.cam_mul[i];
        for (int j = 0; j < 3; j++)
            rgb_cam[i * 3 + j] = (double)lr->color.rgb_cam[i][j];
        /* A non-zero 4th column means a four-colour sensor: the 3x3 matrix would
           not describe it, so the caller must use the black-body fallback. */
        if (fabs((double)lr->color.rgb_cam[i][3]) > 1e-6) { libraw_close(lr); return 0; }
    }
    libraw_close(lr);
    return 1;
}

/* ---- metadata ----------------------------------------------------------- */

static void copy_str(char *dst, size_t n, const char *src)
{
    size_t i = 0;
    for (; i + 1 < n && src[i]; i++) dst[i] = src[i];
    dst[i] = 0;
}

int awpr_read_meta(const char *path, awpr_meta *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    libraw_data_t *lr = libraw_init(0);
    if (!lr) return 0;
    if (open_utf8(lr, path) != LIBRAW_SUCCESS) { libraw_close(lr); return 0; }

    copy_str(out->make, sizeof out->make, lr->idata.make);
    copy_str(out->model, sizeof out->model, lr->idata.model);
    copy_str(out->lens, sizeof out->lens, lr->lens.Lens);
    if (!out->lens[0]) copy_str(out->lens, sizeof out->lens, lr->lens.makernotes.Lens);
    out->iso_speed = lr->other.iso_speed;
    out->shutter   = lr->other.shutter;
    out->aperture  = lr->other.aperture;
    out->focal_len = lr->other.focal_len;
    out->timestamp = (long long)lr->other.timestamp;
    out->flip      = lr->sizes.flip;
    int w = lr->sizes.width, h = lr->sizes.height;
    if (out->flip == 5 || out->flip == 6) { int t = w; w = h; h = t; }
    out->width = w;
    out->height = h;
    libraw_close(lr);
    return 1;
}

/* ---- decode ------------------------------------------------------------- */

static void fill_image(awpr_image *out, libraw_data_t *lr, libraw_processed_image_t *img)
{
    out->opaque    = img;
    out->owner     = (awpr_raw)lr;
    out->data      = img->data;
    out->type      = (int)img->type;
    out->width     = img->width;
    out->height    = img->height;
    out->colors    = img->colors;
    out->bits      = img->bits;
    out->data_size = (int)img->data_size;
}

int awpr_decode_full(const char *path, int bps, awpr_image *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));

    libraw_data_t *lr = libraw_init(0);
    if (!lr) return 0;

    libraw_set_output_color(lr, 1);      /* sRGB                              */
    libraw_set_output_bps(lr, bps);
    libraw_set_no_auto_bright(lr, 0);    /* auto-brightness stays on (see spec)*/

    if (open_utf8(lr, path) != LIBRAW_SUCCESS) goto fail;
    if (libraw_unpack(lr) != LIBRAW_SUCCESS) goto fail;
    if (libraw_dcraw_process(lr) != LIBRAW_SUCCESS) goto fail;

    int err = 0;
    libraw_processed_image_t *img = libraw_dcraw_make_mem_image(lr, &err);
    if (!img) goto fail;
    if (img->type != LIBRAW_IMAGE_BITMAP || img->colors < 3 ||
        img->width <= 0 || img->height <= 0) {
        libraw_dcraw_clear_mem(img);
        goto fail;
    }
    fill_image(out, lr, img);
    return 1;

fail:
    libraw_close(lr);
    return 0;
}

int awpr_decode_linear(const char *path, awpr_image *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));

    libraw_data_t *lr = libraw_init(0);
    if (!lr) return 0;

    libraw_set_output_color(lr, 0);      /* raw camera colour: no matrix       */
    libraw_set_output_bps(lr, 16);
    libraw_set_gamma(lr, 0, 1.0f);       /* linear                             */
    libraw_set_gamma(lr, 1, 1.0f);
    libraw_set_no_auto_bright(lr, 1);
    for (int i = 0; i < 4; i++)          /* no white balance: scaled so the    */
        libraw_set_user_mul(lr, i, 1.0f);/* sensor clip lands on 65535         */

    if (open_utf8(lr, path) != LIBRAW_SUCCESS) goto fail;
    if (lr->idata.colors != 3) goto fail; /* four-colour sensors: not linear-capable */
    if (libraw_unpack(lr) != LIBRAW_SUCCESS) goto fail;
    if (libraw_dcraw_process(lr) != LIBRAW_SUCCESS) goto fail;

    int err = 0;
    libraw_processed_image_t *img = libraw_dcraw_make_mem_image(lr, &err);
    if (!img) goto fail;
    if (img->type != LIBRAW_IMAGE_BITMAP || img->colors < 3 || img->bits != 16 ||
        img->width <= 0 || img->height <= 0) {
        libraw_dcraw_clear_mem(img);
        goto fail;
    }
    fill_image(out, lr, img);
    return 1;

fail:
    libraw_close(lr);
    return 0;
}

int awpr_decode_thumb(const char *path, awpr_image *out, int *flip)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    if (flip) *flip = 0;

    libraw_data_t *lr = libraw_init(0);
    if (!lr) return 0;
    if (open_utf8(lr, path) != LIBRAW_SUCCESS) goto fail;

    /* Embedded previews are usually stored un-rotated and without an orientation
       tag, so the camera's flip is what turns them upright. */
    if (flip) {
        int f = lr->sizes.flip;
        *flip = (f == 3 || f == 5 || f == 6) ? f : 0;
    }

    if (libraw_unpack_thumb(lr) != LIBRAW_SUCCESS) goto fail;

    int err = 0;
    libraw_processed_image_t *img = libraw_dcraw_make_mem_thumb(lr, &err);
    if (!img) goto fail;
    fill_image(out, lr, img);
    return 1;

fail:
    libraw_close(lr);
    return 0;
}

void awpr_free_image(awpr_image *img)
{
    if (!img) return;
    if (img->opaque) libraw_dcraw_clear_mem((libraw_processed_image_t *)img->opaque);
    if (img->owner)  libraw_close((libraw_data_t *)img->owner);
    memset(img, 0, sizeof(*img));
}
