use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::anyhow;
use log::{debug, error, warn};
use prost::Message;
use vigem_client::XButtons;
use webrtc::data_channel::RTCDataChannel;

use webmote_rs::webmote::proto;
use webmote_rs::webmote::proto::update::Update;
use webmote_rs::webmote::proto::{Axis, Button};

use crate::controllers::joystick::Joystick;

pub struct ChannelHandler {
    closed: AtomicBool,
    pub channel: Arc<RTCDataChannel>,
}

impl Drop for ChannelHandler {
    fn drop(&mut self) {
        self.close()
    }
}

impl ChannelHandler {
    pub fn new(channel: Arc<RTCDataChannel>) -> Self {
        Self {
            closed: AtomicBool::new(false),
            channel,
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let joystick =
            Joystick::connect().map_err(|e| anyhow!("Failed to connect to joystick. {}", e))?;

        self.channel.on_message(Box::new(move |data| {
            let joystick = joystick.clone();
            Box::pin(async move {
                let result1 = proto::Update::decode(data.data);
                error!("AAAAAAAAAAA {:?}", result1);
                let Ok(proto::Update {
                    update: Some(update),
                }) = result1
                else {
                    return;
                };

                let result = joystick
                    .update(|gamepad| {
                        match update {
                            Update::Button(button) => {
                                let value = match proto::Button::from_i32(button.name).unwrap() {
                                    Button::Start => XButtons::START,
                                    Button::Back => XButtons::BACK,
                                    Button::LeftThumb => XButtons::LTHUMB,
                                    Button::RightThumb => XButtons::RTHUMB,
                                    Button::LeftShoulder => XButtons::LB,
                                    Button::RightShoulder => XButtons::RB,
                                    Button::Guide => XButtons::GUIDE,
                                    Button::A => XButtons::A,
                                    Button::B => XButtons::B,
                                    Button::X => XButtons::X,
                                    Button::Y => XButtons::Y,
                                };
                                if button.pressed {
                                    gamepad.buttons.raw |= value;
                                } else {
                                    gamepad.buttons.raw &= !value;
                                }
                            }
                            Update::Axis(axis) => {
                                // convert float to -32768..32767
                                let thumb = || {
                                    (
                                        (axis.x * i16::MAX as f32) as i16,
                                        (axis.y * i16::MAX as f32) as i16,
                                    )
                                };
                                let trigger = || (axis.x * u8::MAX as f32) as u8;
                                match proto::Axis::from_i32(axis.name).unwrap() {
                                    Axis::Left => {
                                        (gamepad.thumb_lx, gamepad.thumb_ly) = thumb();
                                    }
                                    Axis::Right => {
                                        (gamepad.thumb_rx, gamepad.thumb_ry) = thumb();
                                    }
                                    Axis::LeftTrigger => gamepad.left_trigger = trigger(),
                                    Axis::RightTrigger => gamepad.right_trigger = trigger(),
                                    Axis::Dpad => {
                                        let horizontal = match axis.x {
                                            d if d > 0.5 => XButtons::RIGHT,
                                            d if d < -0.5 => XButtons::LEFT,
                                            _ => 0,
                                        };
                                        let vertical = match axis.y {
                                            d if d > 0.5 => XButtons::DOWN,
                                            d if d < -0.5 => XButtons::UP,
                                            _ => 0,
                                        };
                                        let button = gamepad.buttons.raw
                                            & !(XButtons::UP
                                                | XButtons::DOWN
                                                | XButtons::LEFT
                                                | XButtons::RIGHT);
                                        gamepad.buttons.raw = button | horizontal | vertical;
                                    }
                                }
                            }
                        }
                    })
                    .await;
                if let Err(err) = result {
                    error!("Failed to update joystick. {}", err);
                }
            })
        }));

        let semaphore = Arc::new(tokio::sync::Semaphore::new(0));
        {
            let semaphore = semaphore.clone();
            self.channel.on_close(Box::new(move || {
                debug!("Data channel closed");
                let semaphore = semaphore.clone();
                Box::pin(async move {
                    semaphore.add_permits(1);
                })
            }));
        }

        semaphore.acquire().await.unwrap().forget();

        Ok(())
    }

    pub fn close(&self) {
        if self.closed.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let channel = self.channel.clone();
        tokio::spawn(async move {
            if let Err(err) = channel.close().await {
                warn!("Failed to close data channel. {}", err);
            }
        });
    }
}
