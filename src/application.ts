import clear from 'clear';
import figlet from 'figlet';
import { program } from 'commander';
import optimizeFiles from './optimize-files';
import readFiles from './read-files';

class Application {
  private static instance: Application;

  private constructor() {}

  public static getInstance(): Application {
    if (!Application.instance) {
        Application.instance = new Application();
    }

    return Application.instance;
  }

  public async main(argv: Array<string>) {
    clear();
    console.log(figlet.textSync('esvg', { horizontalLayout: 'full' }));

    program
      .version('1.0.0')
      .description('SVG Optimization Utility')
      .argument('<glob>', 'The input svg file(s) to optimize')
      .option('-p, --pretty', 'Outputs the SVG in a readable format')
      .parse(argv);

    if (argv.length < 3) program.outputHelp();

    const options = program.opts();

    const files = await readFiles(program.args);

    await optimizeFiles(files, options.pretty);
  }
}

export default Application;
