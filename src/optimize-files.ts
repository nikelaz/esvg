import success from './success';
import optimizeFile from './optimize-file';
import { Options } from './types/options';

async function optimizeFiles(files: Array<string>, options: Options) {
  const operations = files.map(file => optimizeFile(file, options));
  await Promise.all(operations);
  success(files);
}

export default optimizeFiles;
