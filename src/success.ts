import chalk from 'chalk';

function success(files: Array<string>) {
  console.log('\n');
  console.log(chalk.green('Successfully optimized:'));
  files.forEach(file => console.log(file));
}

export default success;
