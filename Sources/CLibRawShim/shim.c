#include "include/CLibRawShim.h"

#include <libraw/libraw.h>
#include <string.h>
#include <math.h>

/* LIBRAW_IMAGE_JPEG = 1, LIBRAW_IMAGE_BITMAP = 2 */

int awpr_libraw_available(void)
{
    libraw_data_t *lr = libraw_init(0);
    if (!lr) return 0;
    libraw_close(lr);
    return 1;
}

const char *awpr_libraw_version(void) { return libraw_version(); }

awpr_raw awpr_open(const char *path)
{
    libraw_data_t *lr = libraw_init(0);
    if (!lr) return NULL;
    if (libraw_open_file(lr, path) != LIBRAW_SUCCESS) { libraw_close(lr); return NULL; }
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
    if (libraw_open_file(lr, path) != LIBRAW_SUCCESS) { libraw_close(lr); return 0; }

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
    if (libraw_open_file(lr, path) != LIBRAW_SUCCESS) { libraw_close(lr); return 0; }

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

    if (libraw_open_file(lr, path) != LIBRAW_SUCCESS) goto fail;
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

int awpr_decode_thumb(const char *path, awpr_image *out, int *flip)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    if (flip) *flip = 0;

    libraw_data_t *lr = libraw_init(0);
    if (!lr) return 0;
    if (libraw_open_file(lr, path) != LIBRAW_SUCCESS) goto fail;

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
