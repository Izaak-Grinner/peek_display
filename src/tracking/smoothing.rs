// 頭の動きをなめらかにする。
// OpenTrack はカメラとは関係なく一定の間隔 (4ms ごと) で送ってくるが、位置が新しくなるのはカメラが
// マーカーを測れたとき (カメラのフレームレート、30回/秒くらい) だけで、その間は同じ位置が繰り返し届く。
// そこで、同じ位置は新しい計測ではないとして飛ばし、計測の間は直前の速さから先の位置を予測し (計測の間隔1回分まで)、
// 1€フィルタ (One Euro Filter) で細かいブレをならす。1€フィルタは、止まっているときは強くならし、
// 速く動くときはほとんどならさないので、ブレが消えても遅れはほとんど増えない。
// 速く動くとカメラの映像がぶれてマーカーを見失い、OpenTrack は最後の位置を送り続ける (位置が変わらなくなる)。
// 見つけ直すと位置が飛ぶので、飛んだときは予測に使わず (行き過ぎないように)、新しい位置まで少しかけて滑らかに動かす。
// 長く見失ったまま (見ている人が離れたなど) なら、正面へゆっくり戻す
use bevy::prelude::*;
use std::f32::consts::TAU;
use std::time::{Duration, Instant};

// 計測の間隔がこれより空いたら、予測しない (見失っているか、トラッキングが止まっている)
const PREDICT_MAX_GAP: Duration = Duration::from_millis(200);

// 位置がこれより長く変わらなければ、マーカーを見失っているとみなす
// (止まっていても、計測のたびに細かいブレで位置は少し変わる)
const LOST_GAP: Duration = Duration::from_millis(150);

// これより速い動き (cm/秒) は、マーカーを見失ってから見つけ直したときの飛びとみなす
const JUMP_SPEED: f32 = 300.0;
// 飛んだときに、新しい位置まで動かす時間
const GLIDE_TIME: Duration = Duration::from_millis(300);

// 見失ってから、最後の位置のまま待つ時間 (速く動いたときのぶれで一瞬見失うのは、ほとんどこれより短い)
const HOLD_TIME: Duration = Duration::from_secs(1);
// 待っても見つからなければ、この時間をかけて正面 (センター合わせの位置, 0) へゆっくり戻す
// (見ている人が離れたとき、映像を斜めのまま止めておかない)
const RETURN_TIME: Duration = Duration::from_millis(1500);

// 1€フィルタの調整値 (位置は cm)。
// MIN_CUTOFF: 止まっているときのならし具合 (Hz。小さいほど強くならす)
// BETA: 速く動くほどならしを弱める度合い
// D_CUTOFF: 速さを求めるときのならし具合 (Hz)
const MIN_CUTOFF: f32 = 1.0;
const BETA: f32 = 0.3;
const D_CUTOFF: f32 = 1.0;

#[derive(Default)]
pub struct PoseSmoother {
    // 最後とその前に計測された (位置が変わった) 位置と時刻
    latest: Option<(Instant, Vec3)>,
    previous: Option<(Instant, Vec3)>,
    filters: [OneEuroFilter; 3],
    last_frame: Option<Instant>,
    // 飛んだあと、新しい位置まで滑らかに動かしている途中なら、その始まり
    glide: Option<Glide>,
    // 最後に使った位置
    last_output: Option<Vec3>,
}

struct Glide {
    from: Vec3,
    start: Instant,
}

impl PoseSmoother {
    // OpenTrack から届いた位置を入れる。新しい計測なら true (前と同じ位置なら false で、何もしない)
    pub fn push(&mut self, at: Instant, position: Vec3) -> bool {
        if let Some((t1, p1)) = self.latest {
            if position == p1 {
                return false;
            }
            let dt = at.duration_since(t1);
            let reacquired = dt > LOST_GAP;
            let jumped = !dt.is_zero() && (position - p1).length() / dt.as_secs_f32() > JUMP_SPEED;
            if reacquired || jumped {
                // 見つけ直した・飛んだ: 前の位置も同じにして速さを 0 にし (予測しない)、今の表示位置から滑らかに動かす
                self.glide = Some(Glide {
                    from: self.last_output.unwrap_or(p1),
                    start: at,
                });
                self.previous = Some((at, position));
                self.latest = Some((at, position));
                return true;
            }
        }
        self.previous = self.latest.replace((at, position));
        true
    }

    // マーカーを見失っているか (位置が LOST_GAP より長く変わっていない)。
    // OpenTrack から届いていないとき (トラッキングが止まっている) と見分けるのは呼ぶ側
    pub fn lost(&self, now: Instant) -> bool {
        self.latest.is_some_and(|(t, _)| now.duration_since(t) > LOST_GAP)
    }

    // このフレームで使う位置 (まだ何も届いていなければ None)
    pub fn update(&mut self, now: Instant) -> Option<Vec3> {
        let mut target = self.predicted(now)?;
        if let Some((t1, _)) = self.latest {
            let waited = now.duration_since(t1).saturating_sub(HOLD_TIME);
            if !waited.is_zero() {
                let t = (waited.as_secs_f32() / RETURN_TIME.as_secs_f32()).min(1.0);
                // ゆっくり動き出し、ゆっくり止まる
                let eased = t * t * (3.0 - 2.0 * t);
                target = target.lerp(Vec3::ZERO, eased);
            }
        }
        if let Some(glide) = &self.glide {
            let t = now.duration_since(glide.start).as_secs_f32() / GLIDE_TIME.as_secs_f32();
            if t < 1.0 {
                // 初めは速く、終わりはゆっくり
                let eased = 1.0 - (1.0 - t) * (1.0 - t);
                target = glide.from.lerp(target, eased);
            } else {
                self.glide = None;
            }
        }
        let dt = self
            .last_frame
            .replace(now)
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(0.0);
        let output = Vec3::new(
            self.filters[0].filter(target.x, dt),
            self.filters[1].filter(target.y, dt),
            self.filters[2].filter(target.z, dt),
        );
        self.last_output = Some(output);
        Some(output)
    }

    // 最後に計測された位置から、直前の速さで今の位置を予測する (計測の間隔1回分まで)
    fn predicted(&self, now: Instant) -> Option<Vec3> {
        let (t1, p1) = self.latest?;
        let Some((t0, p0)) = self.previous else {
            return Some(p1);
        };
        let interval = t1.duration_since(t0);
        let since = now.duration_since(t1);
        if interval.is_zero() || interval > PREDICT_MAX_GAP || since > PREDICT_MAX_GAP {
            return Some(p1);
        }
        let velocity = (p1 - p0) / interval.as_secs_f32();
        Some(p1 + velocity * since.min(interval).as_secs_f32())
    }
}

#[derive(Default)]
struct OneEuroFilter {
    value: Option<f32>,
    derivative: f32,
}

impl OneEuroFilter {
    fn filter(&mut self, x: f32, dt: f32) -> f32 {
        let Some(previous) = self.value else {
            self.value = Some(x);
            return x;
        };
        if dt <= 0.0 {
            return previous;
        }
        let speed = (x - previous) / dt;
        self.derivative += smoothing_factor(D_CUTOFF, dt) * (speed - self.derivative);
        let cutoff = MIN_CUTOFF + BETA * self.derivative.abs();
        let value = previous + smoothing_factor(cutoff, dt) * (x - previous);
        self.value = Some(value);
        value
    }
}

// 遮断周波数 cutoff (Hz) の1次ローパスフィルタの、1ステップ (dt 秒) あたりの係数
fn smoothing_factor(cutoff: f32, dt: f32) -> f32 {
    let tau = 1.0 / (TAU * cutoff);
    1.0 / (1.0 + tau / dt)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 等速で動いているとき、届く間も予測で先に進む
    #[test]
    fn predicts_between_samples() {
        let mut smoother = PoseSmoother::default();
        let t0 = Instant::now();
        let step = Duration::from_millis(33);
        smoother.push(t0, Vec3::ZERO);
        smoother.push(t0 + step, Vec3::new(1.0, 0.0, 0.0));
        let half = smoother.predicted(t0 + step + step / 2).unwrap();
        assert!((half.x - 1.5).abs() < 1e-3, "{}", half.x);
        // 予測は届く間隔1回分まで
        let late = smoother.predicted(t0 + step * 4).unwrap();
        assert!((late.x - 2.0).abs() < 1e-3, "{}", late.x);
        // 間が空きすぎたら予測しない
        let stopped = smoother
            .predicted(t0 + step + Duration::from_millis(300))
            .unwrap();
        assert_eq!(stopped.x, 1.0);
    }

    // 見失ってから見つけ直して位置が飛んだときは、行き過ぎず、すぐには飛ばずに滑らかに動く
    #[test]
    fn glides_after_jump_without_overshoot() {
        let mut smoother = PoseSmoother::default();
        let t0 = Instant::now();
        let frame = Duration::from_millis(16);
        smoother.push(t0, Vec3::ZERO);
        smoother.update(t0);
        // 33ms で 20cm (秒速 6m) 飛ぶ
        let jump = t0 + Duration::from_millis(33);
        smoother.push(jump, Vec3::new(20.0, 0.0, 0.0));
        let first = smoother.update(jump + frame).unwrap();
        assert!(first.x > 0.0 && first.x < 15.0, "{}", first.x);
        let mut max: f32 = 0.0;
        for i in 2..40 {
            max = max.max(smoother.update(jump + frame * i).unwrap().x);
        }
        assert!(max <= 20.0 + 1e-3, "{}", max);
        assert!(max > 19.0, "{}", max);
    }

    // 計測の間に同じ位置が繰り返し届いても、計測の間隔で速さを出して予測する
    #[test]
    fn ignores_repeated_positions() {
        let mut smoother = PoseSmoother::default();
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        assert!(smoother.push(t0, Vec3::ZERO));
        // OpenTrack は 4ms ごとに同じ位置を送ってくる
        for i in 1..8 {
            assert!(!smoother.push(t0 + ms(4 * i), Vec3::ZERO));
        }
        assert!(smoother.push(t0 + ms(33), Vec3::new(1.0, 0.0, 0.0)));
        assert!(!smoother.push(t0 + ms(37), Vec3::new(1.0, 0.0, 0.0)));
        let half = smoother.predicted(t0 + ms(33) + ms(33) / 2).unwrap();
        assert!((half.x - 1.5).abs() < 1e-3, "{}", half.x);
    }

    // 位置が変わらなくなったら見失ったとみなし、見つけ直したら滑らかに動かす
    #[test]
    fn detects_lost_and_reacquired() {
        let mut smoother = PoseSmoother::default();
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        smoother.push(t0, Vec3::ZERO);
        smoother.update(t0);
        assert!(!smoother.lost(t0 + ms(100)));
        assert!(smoother.lost(t0 + ms(500)));
        // 見失っている間に動いて、ゆっくりの速さ (10cm を 0.5 秒) でも、見つけ直したら滑らかに動かす
        let back = t0 + ms(500);
        assert!(smoother.push(back, Vec3::new(10.0, 0.0, 0.0)));
        assert!(!smoother.lost(back));
        let first = smoother.update(back + ms(16)).unwrap();
        assert!(first.x < 5.0, "{}", first.x);
    }

    // 見失っても少しの間は最後の位置のままで、長く見失ったら正面へゆっくり戻り、見つけ直したら滑らかに戻る
    #[test]
    fn returns_to_center_after_long_loss() {
        let mut smoother = PoseSmoother::default();
        let t0 = Instant::now();
        let frame = Duration::from_millis(16);
        smoother.push(t0, Vec3::new(20.0, 0.0, 0.0));
        let mut at = t0;
        let mut x_at = |smoother: &mut PoseSmoother, until: Instant| {
            let mut x = 0.0;
            while at < until {
                at += frame;
                x = smoother.update(at).unwrap().x;
            }
            x
        };
        let held = x_at(&mut smoother, t0 + HOLD_TIME);
        assert!((held - 20.0).abs() < 1e-3, "{}", held);
        let returned = x_at(&mut smoother, t0 + HOLD_TIME + RETURN_TIME + Duration::from_secs(2));
        assert!(returned.abs() < 0.1, "{}", returned);
        // 見つけ直したら、正面から新しい位置まで滑らかに動く (飛ばない)
        let back = t0 + Duration::from_secs(6);
        smoother.push(back, Vec3::new(20.0, 0.0, 0.0));
        let first = smoother.update(back + frame).unwrap().x;
        assert!(first > 0.0 && first < 15.0, "{}", first);
    }

    // 止まっているときの細かいブレは小さくなる
    #[test]
    fn filter_reduces_jitter_at_rest() {
        let mut filter = OneEuroFilter::default();
        let dt = 1.0 / 60.0;
        let mut max: f32 = 0.0;
        for i in 0..600 {
            let noisy = if i % 2 == 0 { 0.1 } else { -0.1 };
            let value = filter.filter(noisy, dt);
            if i > 60 {
                max = max.max(value.abs());
            }
        }
        assert!(max < 0.05, "{}", max);
    }
}
