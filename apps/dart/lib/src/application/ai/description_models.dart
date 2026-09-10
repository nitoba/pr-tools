final class PrDescription {
  const PrDescription({required this.title, required this.body});

  final String title;
  final String body;

  @override
  bool operator ==(Object other) {
    return other is PrDescription && other.title == title && other.body == body;
  }

  @override
  int get hashCode => Object.hash(title, body);
}

final class GeneratedDescription {
  const GeneratedDescription({
    required this.description,
    required this.provider,
    required this.model,
  });

  final PrDescription description;
  final String provider;
  final String model;

  @override
  bool operator ==(Object other) {
    return other is GeneratedDescription &&
        other.description == description &&
        other.provider == provider &&
        other.model == model;
  }

  @override
  int get hashCode => Object.hash(description, provider, model);
}
