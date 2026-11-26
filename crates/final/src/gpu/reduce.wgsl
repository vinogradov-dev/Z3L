// The checkpoint's own pass: one band's accumulators, averaged down to the picture a person watches.
//
// The tracer keeps a band's pixels at image resolution because that is what the filter reads at the
// end, and a plate at A4 is eight thousand samples wide: moving that across the bus four times a
// second would cost more than the tracing. So a snapshot is made on the device — one thread per
// snapshot pixel, summing the block of accumulator pixels that pixel stands for — and only the
// display-sized buffer is read back (`crate::gpu::SNAPSHOT_LONG_SIDE`).
//
// The sums arrive as `vec4`: linear RGB summed over the samples, and the number of samples behind
// them. They are added, not averaged, so `crate::image::Image::from_accumulation` divides by the
// count and a pixel that retired early still weighs what it took: a block is an average of its
// pixels, not an average of averages.
//
// Nothing here is scene-dependent: a band of the accumulator, a block size and the row the band
// starts at are the whole input, and the band's rows are whole blocks (the tracer lays them out that
// way), so no snapshot pixel is ever the sum of a part of two bands.

struct Reduce {
    // x: the band's source width, y: its row count, z: the block a snapshot pixel averages,
    // w: the image row the band starts at.
    band: vec4<u32>,
    // x: the snapshot's width, y: its height.
    picture: vec4<u32>,
};

// The band's pixels, four `vec4`s each; the first is the sum and the count.
@group(0) @binding(0) var<storage, read> sums: array<vec4<f32>>;
@group(0) @binding(1) var<uniform> reduce: Reduce;

// The snapshot being assembled from every band of the image, one block sum per pixel.
@group(1) @binding(0) var<storage, read_write> output: array<vec4<f32>>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let block = max(reduce.band.z, 1u);
    let first_row = reduce.band.w;
    let out_x = id.x;
    let out_row = id.y + first_row / block;
    if (out_x >= reduce.picture.x || out_row >= reduce.picture.y) {
        return;
    }
    // The block this snapshot pixel stands for, clipped to the band and to the image's own edge.
    let x0 = out_x * block;
    let x1 = min(x0 + block, reduce.band.x);
    let top = out_row * block;
    let y0 = clamp(top - first_row, 0u, reduce.band.y);
    let y1 = clamp(top + block - first_row, 0u, reduce.band.y);
    var sum = vec4<f32>(0.0);
    var y = y0;
    loop {
        if (y >= y1) {
            break;
        }
        var x = x0;
        loop {
            if (x >= x1) {
                break;
            }
            sum = sum + sums[(y * reduce.band.x + x) * 4u];
            x = x + 1u;
        }
        y = y + 1u;
    }
    output[out_row * reduce.picture.x + out_x] = sum;
}
