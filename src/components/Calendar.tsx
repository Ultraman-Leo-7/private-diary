import { useMemo, useState } from "react";

export function ymd(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];

export default function Calendar({
  datesWithEntries,
  selectedDate,
  onSelect,
}: {
  datesWithEntries: Set<string>;
  selectedDate: string;
  onSelect: (date: string) => void;
}) {
  const [cursor, setCursor] = useState(() => {
    const d = new Date(selectedDate + "T00:00:00");
    return new Date(d.getFullYear(), d.getMonth(), 1);
  });

  const today = ymd(new Date());

  const cells = useMemo(() => {
    const year = cursor.getFullYear();
    const month = cursor.getMonth();
    const first = new Date(year, month, 1);
    // 让周一作为一周第一列：JS getDay() 周日=0
    const lead = (first.getDay() + 6) % 7;
    const daysInMonth = new Date(year, month + 1, 0).getDate();
    const list: (Date | null)[] = [];
    for (let i = 0; i < lead; i++) list.push(null);
    for (let d = 1; d <= daysInMonth; d++) list.push(new Date(year, month, d));
    while (list.length % 7 !== 0) list.push(null);
    return list;
  }, [cursor]);

  function shiftMonth(delta: number) {
    setCursor(new Date(cursor.getFullYear(), cursor.getMonth() + delta, 1));
  }

  return (
    <div className="calendar">
      <div className="cal-header">
        <button className="icon-btn" onClick={() => shiftMonth(-1)} title="上个月">
          ‹
        </button>
        <span>
          {cursor.getFullYear()} 年 {cursor.getMonth() + 1} 月
        </span>
        <button className="icon-btn" onClick={() => shiftMonth(1)} title="下个月">
          ›
        </button>
      </div>
      <div className="cal-grid cal-weekdays">
        {WEEKDAYS.map((w) => (
          <div key={w} className="cal-wd">
            {w}
          </div>
        ))}
      </div>
      <div className="cal-grid">
        {cells.map((d, i) => {
          if (!d) return <div key={i} className="cal-cell empty" />;
          const ds = ymd(d);
          const classes = ["cal-cell"];
          if (ds === selectedDate) classes.push("selected");
          if (ds === today) classes.push("today");
          return (
            <button key={i} className={classes.join(" ")} onClick={() => onSelect(ds)}>
              {d.getDate()}
              {datesWithEntries.has(ds) && <span className="dot" />}
            </button>
          );
        })}
      </div>
    </div>
  );
}
