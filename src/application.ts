class Application {
  private static instance: Application;

  private constructor() {}

  public static getInstance(): Application {
    if (!Application.instance) {
        Application.instance = new Application();
    }

    return Application.instance;
  }

  public main() {
    
  }
}

export default Application;
