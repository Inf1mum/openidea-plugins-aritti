package example;
public class Application {
 static { throwIfStarted(); }
 static void throwIfStarted() { throw new IllegalStateException("Application initialization is forbidden in this fixture"); }
 public static void main(String[] args) { throwIfStarted(); }
}
