-- AeroPulse-NG DuckDB schema.
-- Track history powers post-flight replay; alerts feed incident review.

CREATE TABLE IF NOT EXISTS track_positions (
    ts_ms           BIGINT       NOT NULL,
    icao24          VARCHAR      NOT NULL,
    callsign        VARCHAR,
    class           VARCHAR,
    latitude        DOUBLE       NOT NULL,
    longitude       DOUBLE       NOT NULL,
    altitude_ft     DOUBLE,
    ground_speed_kt DOUBLE,
    course_deg      DOUBLE,
    vertical_rate_fpm DOUBLE,
    squawk          VARCHAR,
    coasting        BOOLEAN,
    sigma_m         DOUBLE
);

CREATE TABLE IF NOT EXISTS stca_alerts (
    triggered_ms   BIGINT      NOT NULL,
    alert_id       VARCHAR     NOT NULL,
    icao_a         VARCHAR     NOT NULL,
    callsign_a     VARCHAR,
    icao_b         VARCHAR     NOT NULL,
    callsign_b     VARCHAR,
    min_horizontal_nm DOUBLE,
    min_vertical_ft   DOUBLE,
    time_to_closest_s DOUBLE
);

CREATE TABLE IF NOT EXISTS weather_observations (
    observed_ms   BIGINT    NOT NULL,
    source        VARCHAR   NOT NULL,   -- 'AWOS' | 'DATIS' | 'METAR' | 'BDS44' | 'BDS45'
    qnh_hpa       DOUBLE,
    wind_dir_deg  DOUBLE,
    wind_speed_kt DOUBLE,
    temperature_c DOUBLE,
    dewpoint_c    DOUBLE,
    visibility_m  DOUBLE,
    raw_text      VARCHAR
);
