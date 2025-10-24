pub fn ensure_texture_sized<'t>(
    device: &wgpu::Device,
    texture: &'t mut Option<wgpu::Texture>,
    expected_config: &wgpu::TextureDescriptor<'_>,
) -> &'t wgpu::Texture {
    let make_tex = || device.create_texture(expected_config);

    // Weird syntax to get around the lack of Polonius.
    let texture = texture.get_or_insert_with(make_tex);

    if texture.size() != expected_config.size
        || texture.mip_level_count() != expected_config.mip_level_count
        || texture.dimension() != expected_config.dimension
        || texture.format() != expected_config.format
        || texture.usage() != expected_config.usage
    {
        *texture = make_tex();
    }

    texture
}
