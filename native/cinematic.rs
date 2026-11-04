// Client-side cinematic subjects. Native server EntityRef APIs reject remote
// levels, so read the client's actual entity collections through this adapter.
use super::entity::{Entity as _, with_entity};
use super::{Game, commands::Target};
use horizon_client::net::flashback_net::engine::{
    cinematic::{EntityId, TargetPose},
    timeline::Vec3,
};
use horizon_core::geometry::Axis;

fn angle(a: f32, b: f32, t: f64) -> f64 {
    f64::from(a) + ((f64::from(b - a) + 180.0).rem_euclid(360.0) - 180.0) * t
}
impl Game<'_> {
    pub(super) fn flashback_target_pose(&self, uuid: EntityId, partial: f32) -> Option<TargetPose> {
        let uuid = horizon_client::lan::playerdata::uuid_ints(uuid.as_u128());
        let id = *self
            .persist
            .uuids
            .iter()
            .find(|(_, value)| **value == uuid)?
            .0;
        let t = f64::from(partial.clamp(0.0, 1.0));
        if let Some(m) = self.mobs.iter().find(|m| m.id == id && !m.removed) {
            return Some(TargetPose {
                position: Vec3 {
                    x: m.previous.x + (m.position.x - m.previous.x) * t,
                    y: m.previous.y + (m.position.y - m.previous.y) * t,
                    z: m.previous.z + (m.position.z - m.previous.z) * t,
                },
                height: f64::from(m.height()),
                eye_height: m.eye_height(),
                yaw: angle(m.previous_yaw, m.yaw, t),
                body_yaw: angle(m.previous_body_yaw, m.body_yaw, t),
                head_yaw: angle(m.previous_head_yaw, m.head_yaw, t),
                pitch: angle(m.previous_head_pitch, m.head_pitch, t),
            });
        }
        let target = self.level_entity(i32::try_from(id).ok()?)?;
        let current = self.target_position(target)?;
        let mut position = Vec3 {
            x: current[0],
            y: current[1],
            z: current[2],
        };
        let mut height = None;
        for handle in self.entity_handles() {
            let found = with_entity!(self,handle,e=> {
                if e.id()==id {
                    let p=e.position();let previous=e.previous;let bounds=e.bounds();
                    Some((Vec3{x:previous.x+(p.x-previous.x)*t,y:previous.y+(p.y-previous.y)*t,z:previous.z+(p.z-previous.z)*t},
                        bounds.max(Axis::Y)-bounds.min(Axis::Y)))
                } else {None}
            });
            if let Some((p, h)) = found {
                position = p;
                height = Some(h);
                break;
            }
        }
        let height = height.or_else(|| match target {
            Target::Frame(id) => self.item_frames.iter().find(|f| f.id == id).map(|f| {
                let (lo, hi) = f.bounds();
                hi[1] - lo[1]
            }),
            Target::Painting(id) => self.paintings.iter().find(|p| p.id == id).map(|p| {
                let (lo, hi) = p.bounds();
                hi[1] - lo[1]
            }),
            Target::Crystal(id) => self.crystals.iter().find(|c| c.id == id).map(|c| {
                let b = c.bounds();
                b.max(Axis::Y) - b.min(Axis::Y)
            }),
            _ => self
                .res
                .mob_types()
                .get(&self.target_type(target))
                .map(|kind| kind.height),
        })?;
        let [yaw, pitch] = self.target_rotation(target);
        Some(TargetPose {
            position,
            height,
            eye_height: self.target_eye_height(target),
            yaw: f64::from(yaw),
            body_yaw: f64::from(yaw),
            head_yaw: f64::from(yaw),
            pitch: f64::from(pitch),
        })
    }
}
