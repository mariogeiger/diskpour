// Draws the box and, over it, the disks as antialiased quads, colored by age or
// from a table of one color per serial number.

struct View {
    clip_per_unit: f32,
    pixels_per_unit: f32,
    radius: f32,
    crossings_per_step: f32,
    now: u32,
    rows: u32,
    period: u32,
}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<storage, read> paint: array<u32>;

// A disk's color walks around the blue, green and red axes of RGB at a constant
// angular speed in age, a quarter turn from each axis to the next and at constant
// length, so three lobes of a cosine share one circle of three quarter turns. Age
// is measured in box crossings at muzzle speed. The numbers are fitted to the
// reference picture, as is the box color.
const QUARTERS_AT_BIRTH: f32 = 23.1 / 90.0;
const QUARTERS_PER_CROSSING: f32 = 45.7 / 90.0;
const COLOR_LENGTH: f32 = 172.0 / 255.0;
const BOX_COLOR: f32 = 19.0 / 255.0;
const HALF_PI: f32 = 1.5707963;

fn corner_of_strip(index: u32) -> vec2<f32> {
    return vec2<f32>(f32(index & 1u), f32(index >> 1u)) * 2.0 - 1.0;
}

// One channel's brightness at `quarters` around the circle, peaking at `peak`.
fn color_lobe(quarters: f32, peak: f32) -> f32 {
    let distance = (fract((quarters - peak) / 3.0 + 0.5) - 0.5) * 3.0;
    return COLOR_LENGTH * cos(clamp(distance, -1.0, 1.0) * HALF_PI);
}

fn color_of_age(steps: u32) -> vec3<f32> {
    let crossings = f32(steps) * view.crossings_per_step;
    let quarters = QUARTERS_AT_BIRTH + QUARTERS_PER_CROSSING * crossings;
    return vec3<f32>(
        color_lobe(quarters, 2.0),
        color_lobe(quarters, 1.0),
        color_lobe(quarters, 0.0),
    );
}

struct DiskVarying {
    @builtin(position) clip: vec4<f32>,
    @location(0) offset_pixels: vec2<f32>,
    @location(1) radius_pixels: f32,
    @location(2) color: vec3<f32>,
}

fn disk_varying(index: u32, position: vec2<i32>, color: vec3<f32>) -> DiskVarying {
    let radius_pixels = view.radius * view.pixels_per_unit;
    // One extra pixel leaves room for the antialiased rim.
    let offset_pixels = corner_of_strip(index) * (radius_pixels + 1.0);
    let clip_per_pixel = view.clip_per_unit / view.pixels_per_unit;
    let center = vec2<f32>(position) * view.clip_per_unit - 1.0;

    var out: DiskVarying;
    out.clip = vec4<f32>(center + offset_pixels * clip_per_pixel, 0.0, 1.0);
    out.offset_pixels = offset_pixels;
    out.radius_pixels = radius_pixels;
    out.color = color;
    return out;
}

@vertex
fn disk_vertex_by_age(
    @builtin(vertex_index) index: u32,
    @location(0) position: vec2<i32>,
    @location(1) serial: u32,
) -> DiskVarying {
    let birth = serial / view.rows * view.period;
    return disk_varying(index, position, color_of_age(view.now - birth));
}

@vertex
fn disk_vertex_by_paint(
    @builtin(vertex_index) index: u32,
    @location(0) position: vec2<i32>,
    @location(1) serial: u32,
) -> DiskVarying {
    return disk_varying(index, position, unpack4x8unorm(paint[serial]).rgb);
}

@fragment
fn disk_fragment(in: DiskVarying) -> @location(0) vec4<f32> {
    let coverage = clamp(in.radius_pixels + 0.5 - length(in.offset_pixels), 0.0, 1.0);
    return vec4<f32>(in.color * coverage, coverage);
}

@vertex
fn box_vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(corner_of_strip(index), 0.0, 1.0);
}

@fragment
fn box_fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(vec3<f32>(BOX_COLOR), 1.0);
}
