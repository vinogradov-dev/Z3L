# 0237. The wheel's step follows the scene the view is in

- Status: Accepted
- Date: 2026-11-23

## Context

`Camera::zoom` scaled the distance by a fixed ratio, `exp(-points · 0.004)`. A notch of a mouse
wheel is 40 points in `egui`, so one notch took about 15 % of the distance, whatever the maker was
looking at. That is the right step while the model fills the frame, and it is the wrong one while the
model is a speck: a camera at the 1 km distance cap shows a 2.2 m wardrobe as one pixel, and coming
back took thirty notches. The owner reported it as the wheel moving by micrometres.

The projection switch could put a camera there too ([0236](0236-a-view-no-distance-shows-is-fitted-not-clamped.md)),
so the way back is the wheel's job, not the switch's.

## Decision

**The step is the larger of two shares, measured against the scene's own frame.** With `visible` the
height the view shows at its target (`Camera::visible_height`) and `fit` the height a fit would show:

```text
gap  = max(0, ln(visible / fit))            # how far outside the frame the view stands
near = points · 0.004                       # the fine share it always had
far  = gap · (1 − exp(−points · ln2 / 40))  # half of the gap per notch
step = max(near, far), and coming in from outside the frame: min(step, gap)
factor = exp(−sign(points) · step)
```

- While the view shows the scene at or inside a fit, `gap` is zero and the step is the fine share: a
  notch moves the image by the same amount everywhere a maker works.
- Outside the frame the step takes a share of the gap, so the gap halves per notch: a camera left a
  kilometre out is back on the model in six or seven notches.
- Coming in stops on the fit rather than past it, so a fling cannot throw the view through the model.
  Coming out is never stopped by this rule; the distance cap is the stop.
- The fit is `home_camera` over the scene's cached bounds — bounds arithmetic, no evaluation — read
  once per wheel frame by `Document::fit_height`. A scene with no frame leaves the view's own height
  as the measure, which is the plain step.

The wheel, `Ctrl`+middle and the armed *Zoom* drag all reach this through `zoom_about_pointer`, as
[0154](0154-the-wheel-zooms-towards-the-pointer.md) has them share one path.

## Consequences

- From the cap the gap is 5.6, so six notches close it to 5 % and the seventh is the frame the first
  event lands on; the frame test asserts eight and the failed count was thirty.
- The fine step is untouched wherever a maker works, which is what the wheel felt like before.
- Zooming out from a framed view keeps the fine step until the view is about 1.4 fits wide and grows
  from there, so leaving is as quick as coming back.
- The step now depends on the scene's size, so the same notch moves a small scene and a large one by
  the same share of *their* frame rather than by the same number of millimetres. That is the point,
  and it is also the cost: the wheel's feel is a property of the scene, not of the wheel.
