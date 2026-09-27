public class Main {
    public static void main(String[] args) {
        Perro toby = new Perro("Toby", 3, "mestizo");
        Gato luna = new Gato("Luna", 2);

        System.out.println(toby.descripcion());
        System.out.println(luna.descripcion());

        Mascota[] mascotas = { toby, luna };
        for (Mascota mascota : mascotas) {
            System.out.println("Jugando con " + mascota.getNombre() + ":");
            mascota.jugar();
        }

        Veterinario veterinario = new Veterinario("Dra. Ruiz");
        veterinario.atender(toby);
        veterinario.atender(luna);
        System.out.println("Pacientes atendidos: " + veterinario.cuantosPacientes());

        if (args.length > 0) {
            System.out.println("Argumentos recibidos: " + String.join(", ", args));
        }
    }
}