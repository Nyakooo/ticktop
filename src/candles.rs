//! Shared candle rasterization for domestic and overseas charts.
use ratatui::{
    style::{Color, Style},
    symbols::Marker,
    widgets::{Dataset, GraphType},
};

pub fn datasets<'a>(
    wick: &'a [(f64, f64)],
    body: &'a [(f64, f64)],
    color: Color,
) -> [Dataset<'a>; 2] {
    // Use one common cell grid in both directions. HalfBlock bodies overwrite
    // the wick with blank half-cells at their endpoints; Braille wicks also
    // quantize x differently and can shift into the adjacent column.
    [
        Dataset::default()
            .data(wick)
            .marker(Marker::Custom('│'))
            .graph_type(GraphType::Line)
            .style(Style::default().fg(color)),
        Dataset::default()
            .data(body)
            .marker(Marker::Custom('█'))
            .graph_type(GraphType::Line)
            .style(Style::default().fg(color)),
    ]
}
