use scene::clock::FrameInputs;

// The window draws whole progressive frames, so every frame is field 0: on the OSD the odd field moves the
// refraction's v by half a line (scene/Rods.cpp, refracted), which a progressive picture shows as a shake. The
// display buffer index keeps alternating as the OSD's double buffer does.
pub fn first_frame(inputs: &mut FrameInputs) {
    inputs.field = 0;
}

pub fn next_frame(inputs: &mut FrameInputs) {
    inputs.field = 0;
    inputs.display_index ^= 1;
}
