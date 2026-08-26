/** Triple-fusion weather contracts — mirror of models.rs weather types. */

export interface WeatherNode {
  latitude: number;
  longitude: number;
  altitude_ft: number;
  wind_dir_deg: number;
  wind_speed_kt: number;
  temperature_c: number;
  pressure_hpa: number;
  /** bit0 AWOS · bit1 ACARS · bit2 Mode-S BDS */
  fusion_sources: number;
  observed_ms: number;
}

export interface WeatherMatrix {
  nodes: WeatherNode[];
  surface_qnh_hpa: number | null;
  surface_wind_dir_deg: number | null;
  surface_wind_speed_kt: number | null;
  surface_temperature_c: number | null;
  surface_dewpoint_c: number | null;
  visibility_m: number | null;
  dust_layer_top_ft: number | null;
  datis_text: string | null;
  metar_text: string | null;
  updated_ms: number;
}

export interface AtmosphericSample {
  wind_dir_deg: number;
  wind_speed_kt: number;
  temperature_c: number;
  pressure_hpa: number;
  density_kg_m3: number;
  confidence: number;
}

/** Source bitmask decode for the fusion provenance column. */
export function fusionSources(mask: number): string[] {
  const out: string[] = [];
  if (mask & 0b001) out.push("AWOS");
  if (mask & 0b010) out.push("ACARS");
  if (mask & 0b100) out.push("BDS");
  return out;
}
