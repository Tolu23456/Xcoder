struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) rect_pos: vec2<f32>,
    @location(3) rect_size: vec2<f32>,
    @location(4) corner_radius: f32,
    @location(5) border_width: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) rect_pos: vec2<f32>,
    @location(2) rect_size: vec2<f32>,
    @location(3) corner_radius: f32,
    @location(4) border_width: f32,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(model.position, 0.0, 1.0);
    out.color = model.color;
    out.rect_pos = model.rect_pos;
    out.rect_size = model.rect_size;
    out.corner_radius = model.corner_radius;
    out.border_width = model.border_width;
    return out;
}

fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.rect_size * 0.5;
    let dist = sd_rounded_box(in.rect_pos, half_size, in.corner_radius);

    let smoothing = fwidth(dist);
    let alpha = 1.0 - smoothstep(-smoothing, smoothing, dist);

    if (alpha <= 0.0) {
        discard;
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
