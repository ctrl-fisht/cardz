import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  CheckCircle2,
  Keyboard,
  Loader2,
  PartyPopper,
  RotateCcw,
} from "lucide-react";
import { api } from "../lib/api";
import { Markdown } from "../lib/markdown";
import { renderTemplate } from "../lib/template";
import type { Deck, ReviewCard } from "../lib/types";

const RATINGS = [
  { value: 1, key: "again", label: "Снова", className: "again" },
  { value: 2, key: "hard", label: "Трудно", className: "hard" },
  { value: 3, key: "good", label: "Хорошо", className: "good" },
  { value: 4, key: "easy", label: "Легко", className: "easy" },
] as const;

export function Review({
  deck,
  onExit,
  onReviewed,
}: {
  deck: Deck;
  onExit: () => void;
  onReviewed: () => void;
}) {
  const [card, setCard] = useState<ReviewCard | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [answered, setAnswered] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [reviewed, setReviewed] = useState(0);
  const [toast, setToast] = useState<string | null>(null);
  const busyRef = useRef(false);
  const toastTimer = useRef<number | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await api.nextCard(deck.id);
      setCard(next);
      setRevealed(false);
      setAnswered(null);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [deck.id]);

  useEffect(() => {
    void load();
    return () => {
      if (toastTimer.current) window.clearTimeout(toastTimer.current);
    };
  }, [load]);

  const answer = useCallback(
    async (rating: number) => {
      if (!card || busyRef.current) return;
      busyRef.current = true;
      setAnswered(rating);
      try {
        const result = await api.answerCard(card.cardId, rating);
        setReviewed((value) => value + 1);
        onReviewed();
        setToast(`Следующее повторение: ${result.label.toLowerCase()}`);
        if (toastTimer.current) window.clearTimeout(toastTimer.current);
        toastTimer.current = window.setTimeout(() => setToast(null), 1600);
        await new Promise((resolve) => window.setTimeout(resolve, 180));
        await load();
      } catch (err) {
        setError(String(err));
      } finally {
        busyRef.current = false;
      }
    },
    [card, load, onReviewed],
  );

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (!card || busyRef.current) return;
      if (!revealed && (event.code === "Space" || event.key === "Enter")) {
        event.preventDefault();
        setRevealed(true);
        return;
      }
      if (revealed && ["1", "2", "3", "4"].includes(event.key)) {
        event.preventDefault();
        void answer(Number(event.key));
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [answer, card, revealed]);

  const frontText = card ? renderTemplate(card.front, card.fields) : "";
  const backText = card
    ? renderTemplate(card.back, card.fields, frontText)
    : "";

  return (
    <div className="review">
      <header className="review-top">
        <button className="ghost-btn" onClick={onExit}>
          <ArrowLeft size={16} /> К колоде
        </button>
        <div className="review-title">
          <span className="review-deck">{deck.name}</span>
          {card && <span className="review-card-name">{card.cardName}</span>}
        </div>
        <div className="review-progress">
          {reviewed > 0 ? `${reviewed} ответов` : "Начало сессии"}
        </div>
      </header>

      <div className="review-body">
        {loading && (
          <div className="review-loading">
            <Loader2 size={26} className="spin" />
            <span>Подбираем карточку…</span>
          </div>
        )}

        {!loading && error && (
          <div className="review-error">
            <h3>Что-то пошло не так</h3>
            <p>{error}</p>
            <button className="primary-btn" onClick={() => void load()}>
              <RotateCcw size={16} /> Повторить
            </button>
          </div>
        )}

        {!loading && !error && !card && (
          <div className="review-done">
            <div className="done-icon">
              <PartyPopper size={30} />
            </div>
            <h2>На сегодня всё</h2>
            <p>
              {reviewed > 0
                ? `Вы ответили на ${reviewed} карточек. Следующая порция вернётся по расписанию.`
                : "В этой колоде нет карточек к повторению. Добавьте новые или загляните позже."}
            </p>
            <button className="primary-btn" onClick={onExit}>
              <CheckCircle2 size={16} /> Вернуться к колоде
            </button>
          </div>
        )}

        {!loading && !error && card && (
          <div className={`review-card ${revealed ? "revealed" : ""}`}>
            <div className="review-tags">
              {card.isNew && <span className="new-chip">Новая</span>}
              {card.tags.map((tag) => (
                <span key={tag} className="tag">
                  {tag}
                </span>
              ))}
            </div>

            <Markdown text={frontText} deckId={card.deckId} className="review-front" />

            {revealed ? (
              <div className="review-answer">
                <Markdown
                  text={backText}
                  deckId={card.deckId}
                  className="review-back"
                />
              </div>
            ) : (
              <div className="reveal-hint">
                <Keyboard size={15} />
                Пробел — показать ответ
              </div>
            )}
          </div>
        )}
      </div>

      {!loading && !error && card && (
        <div className="review-actions">
          {!revealed ? (
            <button
              className="primary-btn reveal-btn"
              onClick={() => setRevealed(true)}
            >
              Показать ответ
            </button>
          ) : (
            <div className={`rating-row ${answered !== null ? "answered" : ""}`}>
              {RATINGS.map((rating) => (
                <button
                  key={rating.value}
                  className={`rate-btn ${rating.className} ${
                    answered === rating.value ? "picked" : ""
                  }`}
                  onClick={() => void answer(rating.value)}
                >
                  <span className="rate-key">{rating.value}</span>
                  <span className="rate-label">{rating.label}</span>
                  <span className="rate-interval">
                    {card.intervals[rating.key] ?? "—"}
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
