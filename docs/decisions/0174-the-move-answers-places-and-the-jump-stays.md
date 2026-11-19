# 0174. The move answers places, and the jump stays

- Status: Accepted; the reference record of the move algorithm at `21d94d2`
- Date: 2026-11-19

## Context

The move tool was built through five sessions of the owner's reports: a body passing through another
(0165), a piece that could not reach the free place behind an obstacle (0166), a jump that fired on a
frame where no pause could be seen (0167, 0172), a lost bond and a stale hold (0169), a free slot
refused by an axis walk (0171), and a piece pulled back across a body when the hand drifted a
millimetre (0173). It now behaves the way the owner wants, and the next step — measuring a body by its
own surface instead of its box (X85) — rewrites the core of it.

The owner's instruction: fix the algorithm as the rollback point, not as a commit to return to but as
a record of **how it was good**, so a later session knows what it must not lose. And the decision the
surface work opened: the jump stays for everything.

## Decision

**The tool answers a place, not a path.** Z3L is a drafting tool, not a game and not a simulator: a
maker drawing furniture does not walk a board around an obstacle. When the place the hand names can
hold the piece, the piece goes there — across the material in between — and only a place that cannot
hold it is cut at a face. The jump is kept for every body: a panel, an object, a slab, a turned
solid. The pause and the cursor gate are what make a crossing deliberate, not a rule that forbids it.

**The algorithm as it stands** (each sentence is sealed by the tests named beside it):

1. **One snapshot.** `Project::move_rule` reads the moved boxes, the obstacles and the snap settings
   once at take; the rule is pure and stateless and never touches the document.
   (`a_selection_offers_only_the_axes_every_part_of_it_allows`)
2. **Every selection is the object move**, and a furniture piece keeps its class axes while any other
   body is free on three. (`a_panel_offers_only_its_own_thickness_axis`,
   `the_panel_axis_holds_outside_a_product_and_the_move_snaps_anywhere`)
3. **The destination is asked whole first.** If the piece may rest at the proposal, all three axes
   are taken and no contact is reported. (`a_carried_body_lands_in_the_free_place_beyond_a_neighbour`,
   `a_board_beside_a_standing_one_passes_it_where_the_hand_names_free_space`)
4. **Only a refused destination is walked axis by axis**, each cut measured from the place the piece
   **stands** — never from the start of the drag — and a retry pass may free an axis after the axes
   beside it moved, never lose a face already reached.
   (`a_piece_resting_past_a_body_stays_there_and_is_not_pulled_back_across_it`,
   `a_multi_axis_proposal_slides_along_the_axis_a_contact_leaves_free`)
5. **Contact is by faces for a body and by the world box for a furniture piece**; a product's
   envelope is a shell — a reservation, not material — and a piece inside its own case is nested, not
   stuck. (`a_turned_solid_stops_at_its_own_face_and_not_at_the_box_around_it`,
   `a_carcass_and_a_body_stop_each_other_and_a_piece_in_the_case_does_not`)
6. **A touch is not an overlap** (the document's own micrometre): a piece resting on a face slides
   along it. A pair the document already holds overlapping is the maker's to repair and is not
   policed — until X86, which lets it only come out.
   (`a_solid_body_slides_along_a_face_and_keeps_the_other_axes_free`,
   `a_board_that_already_overlaps_can_be_pulled_out_and_is_never_refused_again`)
7. **A crossing rests on the face first.** A place the rule grants whole beyond a body the straight
   path crosses is held `JUMP_HOLD` = 0.12 s, however fast the hand moved, and while the hand's own
   point stands inside a body the hold keeps no age. A cut answer is the face already and a typed
   number is never held. (`a_jump_is_held_until_the_hand_keeps_asking_for_it`,
   `a_crossing_is_held_even_when_the_hand_is_quick`,
   `a_path_that_passes_a_body_is_named_and_a_slide_along_it_is_not`)
8. **A typed number is the maker's** and neither the rule nor a snap moves it; zero gives the face
   back to the pointer. (`a_typed_zero_gives_the_face_back_to_the_pointer`)
9. **Snaps are the workspace's taste**: planes (X = Y = Z = 0, the grid's floor among them), other
   products' envelopes, and the 1 mm grid, at a 5 mm default tolerance; a body measured by its faces
   is not captured by another part's world box.
   (`a_drag_of_a_solid_body_takes_the_planes_and_the_grid_and_not_another_parts_box`,
   `two_frames_meet_flush_inside_the_tolerance_and_stay_apart_beyond`)
10. **The preview is a placement the write accepts.** The release asks the same predicate, so a drag
    never shows a place the document then refuses.
    (`a_write_that_sinks_a_board_into_its_neighbour_is_refused_and_a_touch_is_not`,
    `a_diagonal_carry_never_lands_inside_a_turned_neighbour`)
11. **The log names the piece and the blocker** — kind, name, box, axes at take; `blocked=`,
    `crossing=`, `waiting=` per carried frame (0168–0170).

## Consequences

- X85 changes *what a face is*, not *what the answer means*: the destination is still asked whole,
  the jump still crosses material, and the hold still rests the piece on the face first. Its tests
  must keep every sentence above green; a sentence that has to change is a new ADR, not a passing
  edit.
- The reference is the tree at `21d94d2`; the plan commits after it changed no code.
- A future "physics" mode — gravity, a ball that falls, material that cannot be passed — is a
  separate product decision, and this record is what it would replace.
