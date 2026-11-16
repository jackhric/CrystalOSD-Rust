use scene::frame::Frame;

pub trait FrameSource<C = ()> {
    fn step(&mut self, context: &mut C);
    fn frame(&mut self) -> &mut Frame;
    fn done(&self) -> bool;
}
