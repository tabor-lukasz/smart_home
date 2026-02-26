CREATE TYPE sensor_type AS ENUM (
    'temperature',
    'humidity',
    'door_open',
    'power_consumption',
    'relay_state',
    'temperature_setpoint'
);

-- Value encoding convention:
--   Numeric readings: stored as (real_value * 100) rounded to i64
--     e.g. 21.45 °C  → 2145
--          60.5  %   → 6050
--          1234.56 W → 123456
--   Boolean readings: false → 0, true → 1
CREATE TABLE sensor_readings (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id   TEXT        NOT NULL,
    sensor_type sensor_type NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    value       BIGINT      NOT NULL,

    CONSTRAINT uq_readings_device_type_time
        UNIQUE (device_id, sensor_type, recorded_at)
);

-- Covers all query patterns:
--   • Scatter chart + time range:
--       WHERE device_id = $1 AND sensor_type = $2
--       AND recorded_at BETWEEN $3 AND $4
--       ORDER BY recorded_at ASC
--   • Latest value per (device_id, sensor_type):
--       DISTINCT ON (device_id, sensor_type)
--       ORDER BY device_id, sensor_type, recorded_at DESC
-- The UNIQUE constraint creates this index implicitly; explicit index
-- controls sort order (DESC on recorded_at) for efficient range scans.
CREATE INDEX idx_readings_device_type_time
    ON sensor_readings (device_id, sensor_type, recorded_at DESC);
