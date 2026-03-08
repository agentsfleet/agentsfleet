type Props = {
  number: string;
  title: string;
  description: string;
};

export default function FeatureSection({ number, title, description }: Props) {
  return (
    <div className="feature-section">
      <span className="feature-number">{number}</span>
      <div className="feature-body">
        <h3>{title}</h3>
        <p>{description}</p>
      </div>
    </div>
  );
}
