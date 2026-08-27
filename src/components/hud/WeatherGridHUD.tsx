/**
 * Display 2 Panel B — Triple-Fusion Offline Weather HUD.
 * Surface block (AWOS priority), upper-air matrix from fused nodes,
 * Harmattan dust-layer estimate and raw D-ATIS/METAR text feed.
 */
import { useTelemetry } from "../../hooks/useFlightTracks";
import { fusionSources } from "../../types/weather";
import { fpmToMs, ktToKmh } from "../../lib/units";

export default function WeatherGridHUD() {
  const snap = useTelemetry(2);
  const w = snap?.weather;

  const nodes = [...(w?.nodes ?? [])]
    .sort((a, b) => a.altitude_ft - b.altitude_ft)
    .slice(-8); // latest band sample per layer

  return (
    <div className="panel">
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
          {nodes.length === 0 && (
            <tr>
              <td colSpan={5} style={{ color: "var(--ap-text-dim)" }}>
                No upper air reports available
              </td>
            </tr>
          )}
          {nodes.map((n, i) => (
            <tr key={`${n.observed_ms}-${i}`}>
              <td>{Math.round(n.altitude_ft * 0.3048)} m</td>
              <td>{Math.round(n.wind_dir_deg)}°</td>
              <td>{ktToKmh(n.wind_speed_kt).toFixed(0)} km/h</td>
              <td>{n.temperature_c.toFixed(1)}</td>
              <td style={{ color: "var(--ap-text-dim)" }}>{fusionSources(n.fusion_sources).join("+")}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2 style={{ marginTop: 14 }}>Aerodrome Reports</h2>
      <div className="raw-text-feed">
        {w?.datis_text ?? "No aerodrome information available"}
        {"\n"}
        {w?.metar_text ? w.metar_text : ""}
      </div>
    </div>
  );
}
