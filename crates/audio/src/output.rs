pub const RATE: u32 = 48000;

pub trait OutputBackend: Send {
    fn available(&self) -> bool;
    fn open(&mut self) -> bool;
    fn close(&mut self);
    fn put(&mut self, interleaved_stereo: &[i16]);
}

pub type Fill = Box<dyn FnMut(&mut [i16], usize) + Send>;

pub struct Output {
    fill: Option<Fill>,
    muted: bool,
    available: bool,
    open: bool,
    backend: Option<Box<dyn OutputBackend>>,
}

impl Output {
    pub fn new(fill: Option<Fill>, muted: bool, backend: Option<Box<dyn OutputBackend>>) -> Output {
        let available = backend.as_ref().is_some_and(|b| b.available());
        let mut output = Output { fill, muted: true, available, open: false, backend: if available { backend } else { None } };
        if output.available && !muted {
            output.set_muted(false);
        }
        output
    }

    pub fn available(&self) -> bool {
        self.available
    }

    pub fn open(&self) -> bool {
        self.open
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn set_muted(&mut self, muted: bool) {
        if muted {
            self.muted = true;
            self.close_device();
            return;
        }
        if !self.available {
            return;
        }
        if !self.open {
            self.open_device();
        }
        self.muted = !self.open;
    }

    fn open_device(&mut self) {
        if let Some(backend) = &mut self.backend {
            self.open = backend.open();
        }
    }

    fn close_device(&mut self) {
        if !self.open {
            return;
        }
        if let Some(backend) = &mut self.backend {
            backend.close();
        }
        self.open = false;
    }

    pub fn feed(&mut self, additional: i32) {
        if additional <= 0 {
            return;
        }
        let mut chunk = vec![0i16; additional as usize / 2];
        if !self.muted
            && let Some(fill) = &mut self.fill
        {
            let frames = chunk.len() / 2;
            fill(&mut chunk, frames);
        }
        if let Some(backend) = &mut self.backend {
            backend.put(&chunk);
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.close_device();
    }
}
