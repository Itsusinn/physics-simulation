@group(0) @binding(0) var<storage, read_write> state: State;
@group(0) @binding(1) var<storage, read_write> history: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: Params;

fn displacement(a: Body, b: Body) -> vec2<f32> {
    return (b.position - a.position) + vec2<f32>(b.position_error_x - a.position_error_x, b.position_error_y - a.position_error_y);
}

fn drift(body: Body, delta: vec2<f32>) -> Body {
    // Compensated summation preserves sub-ULP position changes during close encounters.
    var result = body;
    let low = vec2<f32>(body.position_error_x, body.position_error_y);
    let sum = body.position + delta;
    let recovered = sum - body.position;
    let error = (body.position - (sum - recovered)) + (delta - recovered) + low;
    result.position = sum + error;
    let remainder = error - (result.position - sum);
    result.position_error_x = remainder.x;
    result.position_error_y = remainder.y;
    return result;
}

fn acceleration(bodies: array<Body, 3>) -> array<vec2<f32>, 3> {
    var result: array<vec2<f32>, 3>;
    // Evaluate each pair once, so both bodies receive the same pair force.
    for (var i = 0u; i < 3u; i++) {
        for (var j = i + 1u; j < 3u; j++) {
            let delta = displacement(bodies[i], bodies[j]);
            let r2 = max(dot(delta, delta) + params.physics.y * params.physics.y, 1e-20);
            let factor = params.physics.x * inverseSqrt(r2) / r2;
            result[i] += delta * factor * bodies[j].mass;
            result[j] -= delta * factor * bodies[i].mass;
        }
    }
    return result;
}

fn measure() {
    var energy = 0.0;
    var separation = 1e30;
    var momentum = vec2<f32>(0.0);
    var angular = 0.0;
    for (var i = 0u; i < 3u; i++) {
        let b = state.bodies[i];
        energy += 0.5 * b.mass * dot(b.velocity, b.velocity);
        momentum += b.mass * b.velocity;
        angular += b.mass * (b.position.x * b.velocity.y - b.position.y * b.velocity.x);
        for (var j = i + 1u; j < 3u; j++) {
            let d = displacement(b, state.bodies[j]);
            let r2 = dot(d, d);
            separation = min(separation, sqrt(r2));
            energy -= params.physics.x * b.mass * state.bodies[j].mass /
                sqrt(max(r2 + params.physics.y * params.physics.y, 1e-20));
        }
    }
    state.metrics.z = energy;
    state.metrics.w = separation;
    state.diagnostics = vec4<f32>(momentum, angular, state.diagnostics.w);
}

fn record() {
    for (var i = 0u; i < 3u; i++) {
        history[i * HISTORY + state.counters.x] = vec4<f32>(state.bodies[i].position, state.metrics.x, 0.0);
    }
    state.counters.x = (state.counters.x + 1u) % HISTORY;
    state.counters.y = min(state.counters.y + 1u, HISTORY);
}

@compute @workgroup_size(1)
fn simulate() {
    if (state.counters.w == 0u) {
        measure();
        state.metrics.y = state.metrics.z;
        state.counters.w = 1u;
        record();
    }
    if ((state.counters.w & 4u) != 0u || params.physics.w <= 0.0) { return; }
    state.counters.w = 1u;
    state.counters.z = 0u;
    state.diagnostics.w = params.physics.z;
    var elapsed = 0.0;
    var bodies = state.bodies;
    var accel = acceleration(bodies);
    // One invocation owns all three bodies. This avoids races and workgroup barriers.
    for (var step = 0u; step < 2048u; step++) {
        let remaining = params.physics.w - elapsed;
        if (remaining <= params.physics.w * 1e-6) { break; }
        var dt = min(params.physics.z, remaining);
        if (params.controls.z != 0u) {
            for (var i = 0u; i < 3u; i++) {
                for (var j = i + 1u; j < 3u; j++) {
                    let d = displacement(bodies[i], bodies[j]);
                    let r = sqrt(dot(d, d) + params.physics.y * params.physics.y);
                    let dynamical = sqrt(r * r * r / (params.physics.x * (bodies[i].mass + bodies[j].mass)));
                    let crossing = r / max(length(bodies[j].velocity - bodies[i].velocity), 1e-12);
                    dt = min(dt, 0.02 * min(dynamical, crossing));
                }
            }
        }
        if (!(dt >= 1e-10) || elapsed + dt == elapsed) {
            state.counters.w |= 4u;
            break;
        }
        let previous = bodies;
        for (var i = 0u; i < 3u; i++) {
            bodies[i] = drift(bodies[i], bodies[i].velocity * dt + 0.5 * accel[i] * dt * dt);
        }
        let next_accel = acceleration(bodies);
        var valid = true;
        for (var i = 0u; i < 3u; i++) {
            bodies[i].velocity += 0.5 * (accel[i] + next_accel[i]) * dt;
            // Also rejects NaN: comparisons against NaN are false.
            valid = valid && all(abs(bodies[i].position) < vec2<f32>(1e15)) &&
                all(abs(bodies[i].velocity) < vec2<f32>(1e15));
        }
        if (!valid) {
            bodies = previous;
            state.counters.w |= 4u;
            break;
        }
        accel = next_accel;
        elapsed += dt;
        state.counters.z += 1u;
        state.diagnostics.w = min(state.diagnostics.w, dt);
    }
    if (elapsed < params.physics.w * 0.9999 && (state.counters.w & 4u) == 0u) {
        state.counters.w |= 2u; // Work cap reached: slow simulation, never enlarge the timestep.
    }
    state.bodies = bodies;
    state.metrics.x += elapsed;
    measure();
    if (elapsed > 0.0) { record(); }
}
