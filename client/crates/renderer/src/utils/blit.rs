use glam::USizeVec2;

#[derive(Debug, Copy, Clone)]
pub struct BlitOptions {
    pub src_real_size: USizeVec2,
    pub dest_real_size: USizeVec2,
    pub src_crop_start: USizeVec2,
    pub dest_put_start: USizeVec2,
    pub crop_size: USizeVec2,
}

pub fn blit<'a, T: Copy>(
    src: &'a [T],
    dest: &'a mut [T],
    options: BlitOptions,
) -> anyhow::Result<()> {
    let BlitOptions {
        src_real_size,
        dest_real_size,
        src_crop_start,
        dest_put_start,
        crop_size,
    } = options;

    anyhow::ensure!(src.len() == src_real_size.x.saturating_mul(src_real_size.y));
    anyhow::ensure!(dest.len() == dest_real_size.x.saturating_mul(dest_real_size.y));
    anyhow::ensure!(
        src_crop_start
            .saturating_add(crop_size)
            .cmple(src_real_size)
            .all()
    );
    anyhow::ensure!(
        dest_put_start
            .saturating_add(crop_size)
            .cmple(dest_real_size)
            .all()
    );

    raw_blit(
        &src[src_real_size.x * src_crop_start.y..][src_crop_start.x..]
            [..src_real_size.x * crop_size.y],
        &mut dest[dest_real_size.x * dest_put_start.y..][dest_put_start.x..]
            [..dest_real_size.x * crop_size.y],
        options.src_real_size.x,
        options.dest_real_size.x,
        options.crop_size.x,
    );

    Ok(())
}

fn raw_blit<'a, T: Copy>(
    src: &'a [T],
    dest: &'a mut [T],
    src_stride: usize,
    dst_stride: usize,
    chunk_size: usize,
) {
    for (src_chunk, dst_chunk) in src
        .chunks_exact(src_stride)
        .zip(dest.chunks_exact_mut(dst_stride))
    {
        dst_chunk[..chunk_size].copy_from_slice(src_chunk);
    }
}
