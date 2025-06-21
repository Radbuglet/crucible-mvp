@group(0) @binding(0) var textures: binding_array<texture_2d<f32>>;

const vertices = array(
    // (0, 0)       (1, 0)
    //       1----2
    //       |    |
    //       4----3
    // (0, 1)       (1, 1)

    // Triangle 1
    vec2f(-1., -1.),  // (1)
    vec2f( 1., -1.),  // (2)
    vec2f( 1.,  1.),  // (3)

    // Triangle 2
    vec2f(-1., -1.),  // (1)
    vec2f( 1.,  1.),  // (3)
    vec2f(-1.,  1.),  // (4)
);

struct VertexInput {
    @builtin(vertex_index) vertex_idx: u32,
    @location(0) affine_mat_x: vec2f,
    @location(1) affine_mat_y: vec2f,
    @location(2) affine_trans: vec2f,
    @location(3) clip_start: vec2u,
    @location(4) clip_size: vec2u,
    @location(5) tint: u32,
    @location(6) src_idx: u32,
}

struct FragmentInput {
    @builtin(position) pos: vec4f,
    @location(0) tint: vec4f,
    @location(1) src_idx: u32,
    @location(2) uv: vec2f,
}

@vertex
fn vs_main(in: VertexInput) -> FragmentInput {
    let affine_mat = mat2x2(in.affine_mat_x, in.affine_mat_y);
    let vertex_pos = affine_mat * vertices[in.vertex_idx] + in.affine_trans;

    var out: FragmentInput;
    out.pos = vec4f(vertex_pos, 0., 1.);
    out.tint = vec4f(vec4u(
        in.tint & 0xFF,
        (in.tint >> 8) & 0xFF,
        (in.tint >> 16) & 0xFF,
        (in.tint >> 24) & 0xFF
    )) / 255.;
    out.uv = vec2f((vec2u(vertices[in.vertex_idx]) + vec2u(1) / 2) * in.clip_size);

    return out;
}

@fragment
fn fs_main(in: FragmentInput) -> @location(0) vec4f {
    return textureLoad(textures[0], vec2u(in.uv), 0);
}
