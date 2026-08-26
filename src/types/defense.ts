/** Defense overlay contracts — mirror of models.rs defense types. */

export interface Geofence {
  id: string;
  name: string;
  vertices_deg: [number, number][];
  floor_ft: number;
  ceiling_ft: number;
  active: boolean;
}

export interface GeofenceBreach {
  fence_id: string;
  fence_name: string;
  icao24: string;
  callsign: string;
  entered_ms: number;
}

export interface InterceptSolution {
  target_icao24: string;
  interceptor_icao24: string;
  intercept_heading_deg: number;
  required_speed_kt: number;
  time_to_intercept_s: number;
  initial_bearing_deg: number;
  range_nm: number;
  feasible: boolean;
}
