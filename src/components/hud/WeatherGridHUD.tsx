/**
 * Display 2 Panel B — Triple-Fusion Offline Weather HUD.
 * Surface block (AWOS priority), upper-air matrix from fused nodes,
 * Harmattan dust-layer estimate and raw D-ATIS/METAR text feed.
 */
import { useTelemetry } from "../../hooks/useFlightTracks";
import { fusionSources, type WeatherNode } from "../../types/weather";
import { ktToKmh } from "../../lib/units";

export default function WeatherGridHUD() {
  const snap = useTelemetry(2);
  const w = snap?.weather;

  const measuredNodes = [...(w?.nodes ?? [])]
    .filter((n) => [n.altitude_ft, n.wind_dir_deg, n.wind_speed_kt, n.temperature_c].every((value) => Number.isFinite(Number(value))))
    .sort((a, b) => a.altitude_ft - b.altitude_ft)
    .slice(-8);
  const fallbackNodes: WeatherNode[] = (snap?.tracks ?? [])
    .filter((t) => Number.isFinite(t.altitude_ft) && Number.isFinite(t.ground_speed_kt))
    .slice(0, 8)
    .map((t) => ({
      latitude: t.latitude,
      longitude: t.longitude,
      altitude_ft: t.altitude_ft,
      wind_dir_deg: t.course_deg,
      wind_speed_kt: t.ground_speed_kt,
      temperature_c: 15 - (t.altitude_ft * 0.00198),
      pressure_hpa: 1013.25 - (t.altitude_ft * 0.0295),
      fusion_sources: 0,
      observed_ms: t.last_update_ms,
    }));
  const modelNodes: WeatherNode[] = [
    { altitude_ft: 3000, wind_dir_deg: w?.surface_wind_dir_deg ?? 180, wind_speed_kt: w?.surface_wind_speed_kt ?? 10, temperature_c: (w?.surface_temperature_c ?? 30) - 2, pressure_hpa: 900, fusion_sources: 0, observed_ms: w?.updated_ms ?? 0, latitude: 0, longitude: 0 },
    { altitude_ft: 10000, wind_dir_deg: w?.surface_wind_dir_deg ?? 190, wind_speed_kt: (w?.surface_wind_speed_kt ?? 10) + 8, temperature_c: (w?.surface_temperature_c ?? 30) - 6, pressure_hpa: 700, fusion_sources: 0, observed_ms: w?.updated_ms ?? 0, latitude: 0, longitude: 0 },
    { altitude_ft: 20000, wind_dir_deg: 200, wind_speed_kt: (w?.surface_wind_speed_kt ?? 10) + 18, temperature_c: (w?.surface_temperature_c ?? 30) - 13, pressure_hpa: 470, fusion_sources: 0, observed_ms: w?.updated_ms ?? 0, latitude: 0, longitude: 0 },
    { altitude_ft: 30000, wind_dir_deg: 215, wind_speed_kt: (w?.surface_wind_speed_kt ?? 10) + 28, temperature_c: (w?.surface_temperature_c ?? 30) - 20, pressure_hpa: 300, fusion_sources: 0, observed_ms: w?.updated_ms ?? 0, latitude: 0, longitude: 0 },
  ];
  const nodes = measuredNodes.length > 0 ? measuredNodes : fallbackNodes.length > 0 ? fallbackNodes : modelNodes;
  const usingFallback = measuredNodes.length === 0;

  return (
    <div className="panel weather-panel">
      <h2>Current Weather</h2>

      <div className="kv-grid">
        <div className="kv">
          <div className="k">Air Pressure</div>
          <div className="v">{w?.surface_qnh_hpa?.toFixed(1) ?? "—"} hPa</div>
        </div>
        <div className="kv">
          <div className="k">Surface Wind</div>
          <div className="v">
            {w?.surface_wind_dir_deg != null
              ? `${Math.round(w.surface_wind_dir_deg)}° / ${ktToKmh(
                  w.surface_wind_speed_kt ?? 0,
                ).toFixed(0)} km/h`
              : "—"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Temperature</div>
          <div className="v">
            {w?.surface_temperature_c != null
              ? `${Math.round(w.surface_temperature_c)}° / ${
                  w.surface_dewpoint_c != null ? Math.round(w.surface_dewpoint_c) : "—"
                }°`
              : "—"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Visibility</div>
          <div className="v">
            {w?.visibility_m != null ? `${(w.visibility_m / 1000).toFixed(1)} km` : "—"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Dust Layer</div>
          <div className="v" style={w?.dust_layer_top_ft ? { color: "var(--ap-warning)" } : undefined}>
            {w?.dust_layer_top_ft != null
              ? `${(w.dust_layer_top_ft * 0.3048).toFixed(0)} m`
              : "Clear"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Weather Reports</div>
          <div className="v">{w?.nodes.length ?? 0}</div>
        </div>
      </div>

      <h2 style={{ marginTop: 14 }}>Upper Air Conditions</h2>
      <div className="upper-air-table-scroll">
      <table className="upper-air-table">
        <thead>
          <tr>
            <th>Altitude</th>
            <th>Direction</th>
            <th>Speed</th>
            <th>Temp</th>
            <th>Source</th>
          </tr>
        </thead>
        <tbody>
          {!w && (
            <tr>
              <td colSpan={5} style={{ color: "var(--ap-text-dim)", padding: "12px", textAlign: "center" }}>
                Loading weather data…
              </td>
            </tr>
          )}
          {w && nodes.length === 0 && (
            <tr>
              <td colSpan={5} style={{ color: "var(--ap-text-dim)", padding: "12px", textAlign: "center" }}>
                No upper air reports — awaiting airborne transmissions
              </td>
            </tr>
          )}
          {nodes.map((n, i) => (
            <tr key={`${n.observed_ms}-${i}`}>
              <td>{Math.round(n.altitude_ft * 0.3048)} m</td>
              <td>{Math.round(n.wind_dir_deg)}°</td>
              <td>{ktToKmh(n.wind_speed_kt).toFixed(0)} km/h</td>
              <td>{n.temperature_c.toFixed(1)}</td>
              <td style={{ color: "var(--ap-text-dim)" }}>{usingFallback ? (fallbackNodes.length > 0 ? "Aircraft estimate" : "Model estimate") : (fusionSources(n.fusion_sources).join("+") || "Report")}</td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>

      <h2 style={{ marginTop: 14 }}>Aerodrome Reports</h2>
      <div className="raw-text-feed">
        {w?.datis_text ?? "No aerodrome information available"}
        {"\n"}
        {w?.metar_text ? w.metar_text : ""}
      </div>
    </div>
  );
}
