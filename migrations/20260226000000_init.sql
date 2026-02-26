CREATE TABLE sensor_readings (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id            TEXT NOT NULL,
    recorded_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    temperature          DOUBLE PRECISION,
    humidity             DOUBLE PRECISION,
    door_open            BOOLEAN,
    power_consumption    DOUBLE PRECISION,
    relay_state          BOOLEAN,
    temperature_setpoint DOUBLE PRECISION
);

CREATE INDEX idx_sensor_readings_device_id ON sensor_readings (device_id);
CREATE INDEX idx_sensor_readings_recorded_at ON sensor_readings (recorded_at DESC);
