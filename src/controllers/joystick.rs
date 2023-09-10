use std::sync::Arc;

use tokio::sync::RwLock;
use vigem_client::{Client, TargetId, XGamepad, Xbox360Wired};

#[derive(Clone, Debug)]
pub struct Joystick {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Debug)]
pub struct Inner {
    pub gamepad: XGamepad,
    controller: Xbox360Wired<Client>,
}

impl Joystick {
    pub fn connect() -> Result<Joystick, vigem_client::Error> {
        let client = Client::connect()?;

        let id = TargetId::XBOX360_WIRED;
        let mut target = Xbox360Wired::new(client, id);

        target.plugin()?;

        target.wait_ready()?;

        let inner = Inner {
            gamepad: XGamepad::default(),
            controller: target,
        };

        return Ok(Joystick {
            inner: Arc::new(RwLock::new(inner)),
        });
    }

    pub async fn get(&self) -> XGamepad {
        let lock = self.inner.read().await;
        let gamepad = lock.gamepad;
        return gamepad;
    }

    pub async fn update<T>(&self, update: T) -> Result<(), vigem_client::Error>
    where
        T: FnOnce(&mut XGamepad),
    {
        let mut inner = self.inner.write().await;
        update(&mut inner.gamepad);
        let gamepad = inner.gamepad;
        inner.controller.update(&gamepad)?;
        Ok(())
    }
}
