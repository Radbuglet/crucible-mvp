struct VertexInput {
    @builtin(vertex_index) vertex_idx: u32,
}

struct FragmentInput {
    @builtin(position) pos: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> FragmentInput {
    var out: FragmentInput;
    let x = f32(1 - i32(in.vertex_idx)) * 0.5;
    let y = f32(i32(in.vertex_idx & 1u) * 2 - 1) * 0.5;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: FragmentInput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.3, 0.2, 0.1, 1.0);
}
