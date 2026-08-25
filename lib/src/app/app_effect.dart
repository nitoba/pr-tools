import 'package:better_effect/better_effect.dart';

import 'app_failure.dart';

typedef AppEffect<A extends Object> = Effect<A, AppFailure>;
